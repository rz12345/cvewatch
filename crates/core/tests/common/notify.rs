//! 通知測試共用：本機假 SMTP 伺服器（明文或 SMTPS）、自簽 CA 與 HTTPS 替身伺服器。
//! 只監聽 127.0.0.1，不連外。以 `#[path]` 引入（TUI 測試不需要 TLS 相依）。
#![allow(dead_code)]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;

use rcgen::{
    BasicConstraints, CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair,
    KeyUsagePurpose, SanType,
};
use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer};
use rustls::{ServerConfig, ServerConnection, StreamOwned};

/// 自簽 CA 與其簽發的 `localhost` 伺服器憑證。
pub struct TestPki {
    pub ca_pem: String,
    pub server: Arc<ServerConfig>,
}

pub fn pki() -> TestPki {
    let ca_key = KeyPair::generate().expect("ca key");
    let mut ca = CertificateParams::new(Vec::<String>::new()).expect("ca params");
    ca.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    ca.distinguished_name
        .push(DnType::CommonName, "cvewatch test CA");
    ca.key_usages = vec![
        KeyUsagePurpose::KeyCertSign,
        KeyUsagePurpose::CrlSign,
        KeyUsagePurpose::DigitalSignature,
    ];
    let ca_cert = ca.self_signed(&ca_key).expect("ca cert");
    let issuer = Issuer::new(ca, ca_key);

    let key = KeyPair::generate().expect("server key");
    let mut params = CertificateParams::new(vec!["localhost".to_string()]).expect("params");
    params
        .subject_alt_names
        .push(SanType::IpAddress([127, 0, 0, 1].into()));
    params
        .distinguished_name
        .push(DnType::CommonName, "localhost");
    params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    params.key_usages = vec![
        KeyUsagePurpose::DigitalSignature,
        KeyUsagePurpose::KeyEncipherment,
    ];
    let cert = params.signed_by(&key, &issuer).expect("server cert");

    let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
    let server = ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .expect("versions")
        .with_no_client_auth()
        .with_single_cert(
            vec![CertificateDer::from(cert.der().to_vec())],
            PrivatePkcs8KeyDer::from(key.serialize_der()).into(),
        )
        .expect("server config");
    TestPki {
        ca_pem: ca_cert.pem(),
        server: Arc::new(server),
    }
}

/// 假 SMTP 伺服器的行為。
#[derive(Clone, Default)]
pub struct SmtpOptions {
    /// SMTPS（連線即 TLS）
    pub tls: Option<Arc<ServerConfig>>,
    /// 接受的帳密；None 表示不提供 AUTH
    pub auth: Option<(String, String)>,
}

/// 收到的信件（DATA 內容）。
pub type Inbox = Arc<Mutex<Vec<String>>>;

/// 啟動假 SMTP 伺服器，回傳連接埠與收件匣。
pub fn smtp_server(opts: SmtpOptions) -> (u16, Inbox) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let inbox: Inbox = Arc::default();
    let store = inbox.clone();
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let result = match &opts.tls {
                Some(cfg) => match ServerConnection::new(cfg.clone()) {
                    Ok(conn) => serve_smtp(StreamOwned::new(conn, stream), &opts, &store),
                    Err(_) => Ok(()),
                },
                None => serve_smtp(stream, &opts, &store),
            };
            let _ = result;
        }
    });
    (port, inbox)
}

fn serve_smtp<S: Read + Write>(
    stream: S,
    opts: &SmtpOptions,
    inbox: &Inbox,
) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream);
    reply(&mut reader, "220 localhost ESMTP test\r\n")?;
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            return Ok(());
        }
        let cmd = line.trim_end().to_string();
        let upper = cmd.to_ascii_uppercase();
        if upper.starts_with("EHLO") || upper.starts_with("HELO") {
            let auth = if opts.auth.is_some() {
                "250-AUTH PLAIN LOGIN\r\n"
            } else {
                ""
            };
            reply(
                &mut reader,
                &format!("250-localhost\r\n{auth}250 8BITMIME\r\n"),
            )?;
        } else if upper.starts_with("AUTH PLAIN") {
            let encoded = cmd.split_whitespace().nth(2).unwrap_or_default();
            let ok = opts.auth.as_ref().is_some_and(|(u, p)| {
                let expected = data_encoding::BASE64.encode(format!("\0{u}\0{p}").as_bytes());
                expected == encoded
            });
            let resp = if ok {
                "235 2.7.0 Authentication successful\r\n"
            } else {
                "535 5.7.8 Authentication credentials invalid\r\n"
            };
            reply(&mut reader, resp)?;
        } else if upper.starts_with("DATA") {
            reply(&mut reader, "354 End data with <CR><LF>.<CR><LF>\r\n")?;
            let mut data = String::new();
            loop {
                line.clear();
                if reader.read_line(&mut line)? == 0 {
                    return Ok(());
                }
                if line == ".\r\n" {
                    break;
                }
                data.push_str(&line);
            }
            inbox.lock().expect("inbox").push(data);
            reply(&mut reader, "250 2.0.0 Ok: queued\r\n")?;
        } else if upper.starts_with("QUIT") {
            reply(&mut reader, "221 2.0.0 Bye\r\n")?;
            return Ok(());
        } else {
            reply(&mut reader, "250 2.0.0 Ok\r\n")?;
        }
    }
}

fn reply<S: Read + Write>(reader: &mut BufReader<S>, text: &str) -> std::io::Result<()> {
    let s = reader.get_mut();
    s.write_all(text.as_bytes())?;
    s.flush()
}

/// 解碼收到的信件本文（quoted-printable / base64 由 lettre 決定，這裡只取主旨與本文的可讀部分）。
pub fn subject_of(raw: &str) -> String {
    raw.lines()
        .find_map(|l| l.strip_prefix("Subject: "))
        .unwrap_or_default()
        .to_string()
}

/// HTTPS 替身收到的請求。
#[derive(Debug, Clone, Default)]
pub struct Seen {
    /// `POST /issues.json HTTP/1.1`
    pub line: String,
    pub api_key: Option<String>,
    pub body: String,
}

pub type Requests = Arc<Mutex<Vec<Seen>>>;

/// 依序回應預設結果的 HTTPS 伺服器（每個連線一個請求；用完後重複最後一個）。
pub fn https_server(cfg: Arc<ServerConfig>, script: Vec<(u16, String)>) -> (u16, Requests) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let seen: Requests = Arc::default();
    let log = seen.clone();
    thread::spawn(move || {
        let mut idx = 0usize;
        for stream in listener.incoming().flatten() {
            let Ok(conn) = ServerConnection::new(cfg.clone()) else {
                continue;
            };
            let (status, body) = script
                .get(idx.min(script.len().saturating_sub(1)))
                .cloned()
                .unwrap_or((500, String::new()));
            if serve_http(StreamOwned::new(conn, stream), status, &body, &log).is_ok() {
                idx += 1;
            }
        }
    });
    (port, seen)
}

const CRLF: &str = "\r\n";

fn serve_http(
    stream: StreamOwned<ServerConnection, TcpStream>,
    status: u16,
    body: &str,
    log: &Requests,
) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream);
    let mut seen = Seen::default();
    let mut line = String::new();
    reader.read_line(&mut line)?;
    seen.line = line.trim_end().to_string();
    let mut length = 0usize;
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 || line == CRLF {
            break;
        }
        let lower = line.to_ascii_lowercase();
        if let Some(v) = lower.strip_prefix("content-length:") {
            length = v.trim().parse().unwrap_or(0);
        }
        if lower.starts_with("x-redmine-api-key:") {
            seen.api_key = line.split_once(':').map(|(_, v)| v.trim().to_string());
        }
    }
    let mut buf = vec![0; length];
    reader.read_exact(&mut buf)?;
    seen.body = String::from_utf8_lossy(&buf).into_owned();
    log.lock().expect("log").push(seen);
    let headers = [
        format!("HTTP/1.1 {status} X"),
        "content-type: application/json".to_string(),
        format!("content-length: {}", body.len()),
        "connection: close".to_string(),
    ];
    let resp = format!("{}{CRLF}{CRLF}{body}", headers.join(CRLF));
    let s = reader.get_mut();
    s.write_all(resp.as_bytes())?;
    s.flush()?;
    s.conn.send_close_notify();
    let _ = s.flush();
    Ok(())
}
