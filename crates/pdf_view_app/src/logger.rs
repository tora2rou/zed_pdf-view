//! 標準エラーへのログ出力。gpuiがURLを開けなかったときなどのエラーが、端末に見えるようにする。
//!
//! 既定では、どのcrateのものも警告以上だけを出す。環境変数`PDF_VIEW_LOG`に
//! `error`/`warn`/`info`/`debug`/`trace`/`off`を指定すると、このアプリ自身(`pdf_`で始まるもの)の
//! ログを、その詳細度まで出す(gpuiなど他のcrateは、警告以上のまま)。

use log::{Level, LevelFilter, Log, Metadata, Record};

const ENV_VAR: &str = "PDF_VIEW_LOG";

/// 環境変数の値から、このアプリ自身のログの詳細度を決める。未指定・不明な値は警告以上。
fn app_level(value: Option<&str>) -> LevelFilter {
    value
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(LevelFilter::Warn)
}

/// ログを出すか。他のcrateは警告以上、このアプリ自身(`pdf_`で始まる)は`app_level`まで。
fn should_log(app_level: LevelFilter, level: Level, target: &str) -> bool {
    if target.starts_with("pdf_") {
        level <= app_level
    } else {
        level <= Level::Warn
    }
}

struct StderrLogger {
    app_level: LevelFilter,
}

impl Log for StderrLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        should_log(self.app_level, metadata.level(), metadata.target())
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            eprintln!("[{} {}] {}", record.level(), record.target(), record.args());
        }
    }

    fn flush(&self) {}
}

/// ログ出力を有効にする。二重に呼んでも害はない。
pub fn init() {
    let level = app_level(std::env::var(ENV_VAR).ok().as_deref());
    let logger = Box::leak(Box::new(StderrLogger { app_level: level }));
    if log::set_logger(logger).is_ok() {
        // 他のcrateの警告以上も通すので、最大は少なくとも警告にしておく。
        log::set_max_level(level.max(LevelFilter::Warn));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_defaults_to_warn() {
        assert_eq!(app_level(None), LevelFilter::Warn);
        assert_eq!(app_level(Some("nonsense")), LevelFilter::Warn);
        assert_eq!(app_level(Some("")), LevelFilter::Warn);
    }

    #[test]
    fn level_is_parsed_case_insensitively() {
        assert_eq!(app_level(Some("debug")), LevelFilter::Debug);
        assert_eq!(app_level(Some(" INFO ")), LevelFilter::Info);
        assert_eq!(app_level(Some("off")), LevelFilter::Off);
    }

    #[test]
    fn warnings_from_any_crate_are_shown_by_default() {
        let warn = LevelFilter::Warn;
        assert!(should_log(
            warn,
            Level::Error,
            "gpui_linux::linux::platform"
        ));
        assert!(should_log(warn, Level::Warn, "gpui"));
        assert!(!should_log(warn, Level::Info, "pdf_viewer::link_nav"));
        assert!(!should_log(warn, Level::Debug, "pdf_viewer::link_nav"));
    }

    #[test]
    fn debug_shows_only_this_apps_details() {
        let debug = LevelFilter::Debug;
        assert!(should_log(debug, Level::Debug, "pdf_viewer::link_nav"));
        assert!(should_log(debug, Level::Info, "pdf_view_app"));
        // gpuiやwgpuの詳細は、debugにしても出さない(多すぎるため)
        assert!(!should_log(debug, Level::Debug, "gpui::window"));
        assert!(!should_log(debug, Level::Info, "wgpu_hal"));
    }

    #[test]
    fn off_hides_this_apps_logs_but_not_other_crates_errors() {
        let off = LevelFilter::Off;
        assert!(!should_log(off, Level::Warn, "pdf_viewer"));
        assert!(should_log(off, Level::Error, "gpui_linux"));
    }
}
