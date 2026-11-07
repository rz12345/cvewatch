//! 子程序與錯誤訊息的共用小工具。

use std::process::Command;

/// Windows `CREATE_NO_WINDOW`：子程序不開主控台視窗。
pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// 設定子程序不開主控台視窗（非 Windows 不做事）。
pub fn hide_window(cmd: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// 錯誤與其來源鏈，以「: 」串接；略過已包含在前面訊息中的重複文字。
pub fn error_chain(err: &dyn std::error::Error) -> String {
    let mut parts = vec![err.to_string()];
    let mut source = err.source();
    while let Some(s) = source {
        let text = s.to_string();
        if !parts.iter().any(|p| p.contains(&text)) {
            parts.push(text);
        }
        source = s.source();
    }
    parts.join(": ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct E(&'static str, Option<Box<E>>);

    impl std::fmt::Display for E {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str(self.0)
        }
    }

    impl std::error::Error for E {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            self.1.as_deref().map(|e| e as _)
        }
    }

    #[test]
    fn chain_skips_repeated_text() {
        let e = E(
            "connect: refused",
            Some(Box::new(E("refused", Some(Box::new(E("os 10061", None)))))),
        );
        assert_eq!(error_chain(&e), "connect: refused: os 10061");
    }
}
