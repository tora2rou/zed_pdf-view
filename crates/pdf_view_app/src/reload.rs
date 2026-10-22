//! 表示中のPDFが書き換わったら、自動で読み直す。

use std::{
    fs,
    path::Path,
    time::{Duration, SystemTime},
};

use gpui::{AppContext as _, Context, Window};
use pdf_core::PdfDocument;
use std::sync::Arc;

use crate::app_window::AppWindow;

/// ファイルが変わっていないか調べる間隔。
pub const POLL_INTERVAL: Duration = Duration::from_millis(500);

/// ファイルの状態の目印(更新時刻とサイズ)。これが変わったら、書き換わったとみなす。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileStamp {
    modified: Option<SystemTime>,
    len: u64,
}

impl FileStamp {
    /// `path`の今の目印。ファイルが無い(書き換えの途中で消えているなど)・読めないときは`None`。
    pub fn of(path: &Path) -> Option<Self> {
        let meta = fs::metadata(path).ok()?;
        Some(Self {
            modified: meta.modified().ok(),
            len: meta.len(),
        })
    }
}

/// 確認の結果、次にすること。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// 変わっていない。何もしない。
    Idle,
    /// 変わったが、書き込み途中かもしれない。次の確認でも同じなら読み込む。
    Wait,
    /// 変わったまま落ち着いたので、読み込む。
    Reload,
}

/// 確認1回分の判断。`loaded`は表示中の文書を読んだときの目印、`pending`は前回の確認で見た
/// (まだ読み込んでいない)目印、`current`は今の目印。返す2つ目は、次回の`pending`。
///
/// TypstやLaTeXはPDFを書き出している最中のことがあるので、目印が2回続けて同じになるまで待つ。
pub fn step(
    loaded: Option<FileStamp>,
    pending: Option<FileStamp>,
    current: Option<FileStamp>,
) -> (Step, Option<FileStamp>) {
    match current {
        None => (Step::Wait, None),
        Some(now) if Some(now) == loaded => (Step::Idle, None),
        Some(now) if Some(now) == pending => (Step::Reload, None),
        Some(now) => (Step::Wait, Some(now)),
    }
}

impl AppWindow {
    /// 表示中のタブのファイルを、一定の間隔で確認し続けるタスク。ウィンドウが閉じたら終わる。
    pub(crate) fn watch_active_file(window: &mut Window, cx: &mut Context<Self>) -> gpui::Task<()> {
        cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor().timer(POLL_INTERVAL).await;
                let Ok(Some(path)) = this.update(cx, |this, _| this.active_path()) else {
                    if this.upgrade().is_none() {
                        return;
                    }
                    continue;
                };
                let target = path.clone();
                let current = cx
                    .background_spawn(async move { FileStamp::of(&target) })
                    .await;
                let Ok(decision) = this.update(cx, |this, _| this.decide_reload(&path, current))
                else {
                    return;
                };
                if decision != Some(Step::Reload) {
                    continue;
                }
                let target = path.clone();
                let loaded = cx
                    .background_spawn(async move { PdfDocument::open(&target) })
                    .await;
                let result = this.update_in(cx, |this, window, cx| {
                    this.finish_reload(&path, current, loaded.ok().map(Arc::new), window, cx)
                });
                if result.is_err() {
                    return;
                }
            }
        })
    }

    /// 表示中のタブのファイルの場所。
    fn active_path(&self) -> Option<std::path::PathBuf> {
        self.tabs.get(self.active).map(|tab| tab.path.clone())
    }

    /// `path`のタブについて確認1回分の判断をし、次回に向けた状態を更新する。タブが無ければ`None`。
    fn decide_reload(&mut self, path: &Path, current: Option<FileStamp>) -> Option<Step> {
        let tab = self.tabs.iter_mut().find(|tab| tab.path == path)?;
        let (decision, pending) = step(tab.loaded, tab.pending, current);
        tab.pending = pending;
        Some(decision)
    }

    /// 読み直した文書を、`path`のタブに反映する。読み込めなかったときは、古い表示を残して、
    /// 次の確認でやり直す。
    fn finish_reload(
        &mut self,
        path: &Path,
        stamp: Option<FileStamp>,
        document: Option<Arc<PdfDocument>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(tab) = self.tabs.iter_mut().find(|tab| tab.path == path) else {
            return;
        };
        let Some(document) = document else {
            return;
        };
        tab.loaded = stamp;
        tab.pending = None;
        tab.viewer
            .update(cx, |view, cx| view.reload_document(document, window, cx));
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf, sync::Arc};

    use gpui::TestAppContext;
    use pdf_core::PdfDocument;

    use super::*;

    const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../pdf_core/tests/fixtures");

    fn stamp(len: u64) -> Option<FileStamp> {
        Some(FileStamp {
            modified: None,
            len,
        })
    }

    #[test]
    fn unchanged_file_does_nothing() {
        assert_eq!(step(stamp(1), None, stamp(1)), (Step::Idle, None));
        // 前回「待ち」にした変更が、元に戻っていたら忘れる。
        assert_eq!(step(stamp(1), stamp(2), stamp(1)), (Step::Idle, None));
    }

    /// 変わった直後は待ち、次の確認でも同じ目印なら読み込む(書き込み途中を読まないため)。
    #[test]
    fn a_change_must_settle_before_reloading() {
        let (first, pending) = step(stamp(1), None, stamp(2));
        assert_eq!((first, pending), (Step::Wait, stamp(2)));
        assert_eq!(step(stamp(1), pending, stamp(2)), (Step::Reload, None));
    }

    /// 書き込みが続いて目印が動き続けている間は、読み込まない。
    #[test]
    fn a_file_still_being_written_is_not_reloaded() {
        let (_, pending) = step(stamp(1), None, stamp(2));
        assert_eq!(step(stamp(1), pending, stamp(3)), (Step::Wait, stamp(3)));
    }

    #[test]
    fn a_missing_file_waits_and_forgets_the_pending_change() {
        assert_eq!(step(stamp(1), stamp(2), None), (Step::Wait, None));
    }

    fn temp_pdf(name: &str, from: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pdf_view_app_{name}_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("doc.pdf");
        fs::copy(format!("{FIXTURES}/{from}"), &path).unwrap();
        path
    }

    fn page_count(view: &gpui::Entity<AppWindow>, cx: &gpui::VisualTestContext) -> usize {
        view.read_with(cx, |this, cx| {
            this.tabs[0].viewer.read(cx).document().page_count()
        })
    }

    /// 開いているファイルを書き換えると、しばらくして自動で読み直される。
    #[gpui::test]
    fn a_rewritten_file_is_reloaded_automatically(cx: &mut TestAppContext) {
        let path = temp_pdf("reload", "two_pages.pdf");
        let document = Arc::new(PdfDocument::open(&path).unwrap());
        let (view, cx) = cx.add_window_view(|window, cx| {
            AppWindow::new(Some((path.clone(), document)), None, window, cx)
        });
        cx.executor().allow_parking();
        assert_eq!(page_count(&view, cx), 2);

        fs::copy(format!("{FIXTURES}/with_double_strike.pdf"), &path).unwrap();
        // 1回目の確認で変化に気づき、2回目の確認で読み込む。
        for _ in 0..2 {
            cx.executor().advance_clock(POLL_INTERVAL * 2);
            cx.run_until_parked();
        }
        let mut reloaded = false;
        for _ in 0..200 {
            cx.run_until_parked();
            if page_count(&view, cx) == 1 {
                reloaded = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(reloaded, "the rewritten file was not reloaded");
    }

    /// 書き換えなければ、読み直さない。
    #[gpui::test]
    fn an_untouched_file_is_not_reloaded(cx: &mut TestAppContext) {
        let path = temp_pdf("untouched", "two_pages.pdf");
        let document = Arc::new(PdfDocument::open(&path).unwrap());
        let (view, cx) = cx.add_window_view(|window, cx| {
            AppWindow::new(Some((path.clone(), document)), None, window, cx)
        });
        cx.executor().allow_parking();
        let before = view.read_with(cx, |this, cx| {
            Arc::as_ptr(this.tabs[0].viewer.read(cx).document())
        });
        for _ in 0..4 {
            cx.executor().advance_clock(POLL_INTERVAL * 2);
            cx.run_until_parked();
            std::thread::sleep(Duration::from_millis(20));
        }
        let after = view.read_with(cx, |this, cx| {
            Arc::as_ptr(this.tabs[0].viewer.read(cx).document())
        });
        assert_eq!(before, after, "the document was reloaded without a change");
    }

    /// 読めないファイルに置き換わっても古い表示を残し、読めるようになったら読み直す。
    #[gpui::test]
    fn an_unreadable_file_keeps_the_old_view_until_it_is_fixed(cx: &mut TestAppContext) {
        let path = temp_pdf("broken", "two_pages.pdf");
        let document = Arc::new(PdfDocument::open(&path).unwrap());
        let (view, cx) = cx.add_window_view(|window, cx| {
            AppWindow::new(Some((path.clone(), document)), None, window, cx)
        });
        cx.executor().allow_parking();

        let settle = |cx: &mut gpui::VisualTestContext| {
            for _ in 0..4 {
                cx.executor().advance_clock(POLL_INTERVAL * 2);
                cx.run_until_parked();
                std::thread::sleep(Duration::from_millis(20));
            }
        };
        fs::write(&path, b"%PDF-1.4 truncated").unwrap();
        settle(cx);
        assert_eq!(page_count(&view, cx), 2, "the old view should stay");

        fs::copy(format!("{FIXTURES}/with_double_strike.pdf"), &path).unwrap();
        settle(cx);
        assert_eq!(page_count(&view, cx), 1, "the fixed file should be loaded");
    }
}
