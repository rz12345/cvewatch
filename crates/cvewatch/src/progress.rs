//! 以 indicatif 顯示長時間下載（cvelistV5 baseline）的進度。

use cvewatch_core::ingest::source::Progress;
use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};

const TEMPLATE: &str = "{msg} [{bar:30}] {bytes}/{total_bytes} {bytes_per_sec} 剩餘 {eta}";
const SPINNER_TEMPLATE: &str = "{msg} {spinner} {bytes} {bytes_per_sec}";

/// 終端機進度條；stderr 不是終端機（例如工作排程器）時不顯示。
pub struct BarProgress {
    bar: ProgressBar,
}

impl BarProgress {
    pub fn new() -> Self {
        let bar = ProgressBar::with_draw_target(None, ProgressDrawTarget::stderr());
        bar.finish_and_clear();
        BarProgress { bar }
    }
}

impl Progress for BarProgress {
    fn begin(&self, label: &str, total_bytes: Option<u64>) {
        self.bar.reset();
        let style = match total_bytes {
            Some(total) => {
                self.bar.set_length(total);
                ProgressStyle::with_template(TEMPLATE)
            }
            None => ProgressStyle::with_template(SPINNER_TEMPLATE),
        };
        if let Ok(style) = style {
            self.bar.set_style(style.progress_chars("=> "));
        }
        self.bar.set_message(label.to_string());
    }

    fn advance(&self, bytes: u64) {
        self.bar.inc(bytes);
    }

    fn end(&self) {
        self.bar.finish_and_clear();
    }
}
