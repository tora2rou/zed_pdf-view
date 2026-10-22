//! 表示中のファイル(PDF、またはTypst・LaTeXの文書とその依存ファイル)が書き換わったら、自動で読み直す。

use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime},
};

use gpui::{AppContext as _, Context, Window};
use pdf_core::PdfDocument;

use crate::{app_window::AppWindow, source_tab};

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

/// 見張っているファイル全部の目印。`paths`と同じ順で、ファイルが無いものは`None`。
pub type Snapshot = Vec<Option<FileStamp>>;

pub fn snapshot(paths: &[PathBuf]) -> Snapshot {
    paths.iter().map(|p| FileStamp::of(p)).collect()
}

/// タブが見張っているファイルと、その状態。
pub struct Watch {
    pub paths: Vec<PathBuf>,
    /// 表示中の内容を読んだときの目印。
    pub loaded: Snapshot,
    /// 前回の確認で見た、まだ読み込んでいない目印(書き込み途中でないかの確認用)。
    pub pending: Option<Snapshot>,
}

impl Watch {
    pub fn new(paths: Vec<PathBuf>, loaded: Snapshot) -> Self {
        Self {
            paths,
            loaded,
            pending: None,
        }
    }

    /// 1つのファイルを、今の状態で見張り始める。
    pub fn single(path: &Path) -> Self {
        let paths = vec![path.to_path_buf()];
        let loaded = snapshot(&paths);
        Self::new(paths, loaded)
    }
}

/// ビルドで見張る対象が変わったときの、新しい目印。ビルド前から見張っていたファイルは、ビルド前の目印を
/// 使う(ビルド中に書き換えられても、次の確認で気づけるように)。ビルドで増えたファイルは、ビルド後の
/// 目印を使う。
pub fn merged_snapshot(
    before_paths: &[PathBuf],
    before: &Snapshot,
    new_paths: &[PathBuf],
    after: &Snapshot,
) -> Snapshot {
    new_paths
        .iter()
        .zip(after)
        .map(
            |(path, now)| match before_paths.iter().position(|p| p == path) {
                Some(i) => before[i],
                None => *now,
            },
        )
        .collect()
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

/// 確認1回分の判断。`loaded`は表示中の内容を読んだときの目印、`pending`は前回の確認で見た
/// (まだ読み込んでいない)目印、`current`は今の目印。返す2つ目は、次回の`pending`。
///
/// TypstやLaTeXはPDFを書き出している最中のことがあるので、目印が2回続けて同じになるまで待つ。
pub fn step<T: PartialEq + Clone>(
    loaded: &T,
    pending: &Option<T>,
    current: &T,
) -> (Step, Option<T>) {
    if current == loaded {
        (Step::Idle, None)
    } else if pending.as_ref() == Some(current) {
        (Step::Reload, None)
    } else {
        (Step::Wait, Some(current.clone()))
    }
}

/// 読み直す種類。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Pdf,
    Source,
}

impl AppWindow {
    /// 表示中のタブのファイルを、一定の間隔で確認し続けるタスク。ウィンドウが閉じたら終わる。
    pub(crate) fn watch_active_file(window: &mut Window, cx: &mut Context<Self>) -> gpui::Task<()> {
        cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor().timer(POLL_INTERVAL).await;
                let Ok(Some((path, paths))) = this.update(cx, |this, _| this.active_watch()) else {
                    if this.upgrade().is_none() {
                        return;
                    }
                    continue;
                };
                let targets = paths.clone();
                let current = cx.background_spawn(async move { snapshot(&targets) }).await;
                let Ok(decision) =
                    this.update(cx, |this, _| this.decide_reload(&path, &paths, &current))
                else {
                    return;
                };
                let Some((Step::Reload, kind)) = decision else {
                    continue;
                };
                let alive = match kind {
                    Kind::Pdf => Self::reload_pdf(&this, cx, path, current).await,
                    Kind::Source => Self::rebuild_source(&this, cx, path, paths, current).await,
                };
                if !alive {
                    return;
                }
            }
        })
    }

    /// PDFを読み直して、タブに反映する。ウィンドウが無くなっていたら`false`。
    async fn reload_pdf(
        this: &gpui::WeakEntity<Self>,
        cx: &mut gpui::AsyncWindowContext,
        path: PathBuf,
        current: Snapshot,
    ) -> bool {
        let target = path.clone();
        let loaded = cx
            .background_spawn(async move { PdfDocument::open(&target) })
            .await;
        this.update_in(cx, |this, window, cx| {
            this.finish_reload(&path, current, loaded.ok().map(Arc::new), window, cx)
        })
        .is_ok()
    }

    /// Typst・LaTeXの文書をコンパイルし直して、タブに反映する。ウィンドウが無くなっていたら`false`。
    async fn rebuild_source(
        this: &gpui::WeakEntity<Self>,
        cx: &mut gpui::AsyncWindowContext,
        path: PathBuf,
        before_paths: Vec<PathBuf>,
        before: Snapshot,
    ) -> bool {
        let started = this.update(cx, |this, cx| {
            this.compiling.push(path.clone());
            cx.notify();
        });
        if started.is_err() {
            return false;
        }
        let source = path.clone();
        let (build, merged) = cx
            .background_spawn(async move {
                let build = source_tab::build(&source);
                let after = snapshot(&build.dependencies);
                let merged = merged_snapshot(&before_paths, &before, &build.dependencies, &after);
                (build, merged)
            })
            .await;
        this.update_in(cx, |this, window, cx| {
            this.finish_source(&path, build, merged, window, cx)
        })
        .is_ok()
    }

    /// 表示中のタブの、ファイルの場所と、見張っているファイル。
    fn active_watch(&self) -> Option<(PathBuf, Vec<PathBuf>)> {
        let tab = self.tabs.get(self.active)?;
        Some((tab.path.clone(), tab.watch.paths.clone()))
    }

    /// `path`のタブについて確認1回分の判断をし、次回に向けた状態を更新する。タブが無い、または見張る
    /// 対象が変わっていれば`None`。
    fn decide_reload(
        &mut self,
        path: &Path,
        paths: &[PathBuf],
        current: &Snapshot,
    ) -> Option<(Step, Kind)> {
        let tab = self.tabs.iter_mut().find(|tab| tab.path == path)?;
        if tab.watch.paths != paths {
            return None;
        }
        let kind = if tab.source.is_some() {
            Kind::Source
        } else {
            Kind::Pdf
        };
        // 文書本体が(書き換えの途中などで)無いときは、揃うまで待つ。
        let main = paths.iter().position(|p| p == path);
        if main.is_some_and(|i| current[i].is_none()) {
            tab.watch.pending = None;
            return Some((Step::Wait, kind));
        }
        let (decision, pending) = step(&tab.watch.loaded, &tab.watch.pending, current);
        tab.watch.pending = pending;
        Some((decision, kind))
    }

    /// 読み直したPDFを、`path`のタブに反映する。読み込めなかったときは、古い表示を残して、
    /// 次の確認でやり直す。
    fn finish_reload(
        &mut self,
        path: &Path,
        snapshot: Snapshot,
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
        tab.watch.loaded = snapshot;
        tab.watch.pending = None;
        tab.viewer
            .update(cx, |view, cx| view.reload_document(document, window, cx));
    }

    /// コンパイルし直した結果を、`path`のタブに反映する。コンパイルに失敗したときは、古い表示を残して、
    /// エラーを表示する。見張るファイルは、読み込んだ依存ファイルに置き換える。
    pub(crate) fn finish_source(
        &mut self,
        path: &Path,
        build: source_tab::Build,
        snapshot: Snapshot,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.compiling.retain(|p| p != path);
        cx.notify();
        let Some(tab) = self.tabs.iter_mut().find(|tab| tab.path == path) else {
            return;
        };
        tab.watch = Watch::new(build.dependencies, snapshot);
        if let Some(source) = &mut tab.source {
            source.diagnostics = build.diagnostics;
        }
        if let Some(document) = build.document {
            tab.viewer.update(cx, |view, cx| {
                view.reload_document(Arc::new(document), window, cx)
            });
        }
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf, sync::Arc};

    use gpui::TestAppContext;
    use pdf_core::PdfDocument;

    use super::*;

    const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../pdf_core/tests/fixtures");

    fn stamp(len: u64) -> Snapshot {
        vec![Some(FileStamp {
            modified: None,
            len,
        })]
    }

    #[test]
    fn unchanged_file_does_nothing() {
        assert_eq!(step(&stamp(1), &None, &stamp(1)), (Step::Idle, None));
        // 前回「待ち」にした変更が、元に戻っていたら忘れる。
        assert_eq!(
            step(&stamp(1), &Some(stamp(2)), &stamp(1)),
            (Step::Idle, None)
        );
    }

    /// 変わった直後は待ち、次の確認でも同じ目印なら読み込む(書き込み途中を読まないため)。
    #[test]
    fn a_change_must_settle_before_reloading() {
        let (first, pending) = step(&stamp(1), &None, &stamp(2));
        assert_eq!((first, pending.clone()), (Step::Wait, Some(stamp(2))));
        assert_eq!(step(&stamp(1), &pending, &stamp(2)), (Step::Reload, None));
    }

    /// 書き込みが続いて目印が動き続けている間は、読み込まない。
    #[test]
    fn a_file_still_being_written_is_not_reloaded() {
        let (_, pending) = step(&stamp(1), &None, &stamp(2));
        assert_eq!(
            step(&stamp(1), &pending, &stamp(3)),
            (Step::Wait, Some(stamp(3)))
        );
    }

    /// 複数のファイルのうち1つでも変われば、変更とみなす。
    #[test]
    fn a_change_in_any_watched_file_counts() {
        let one = Some(FileStamp {
            modified: None,
            len: 1,
        });
        let two = Some(FileStamp {
            modified: None,
            len: 2,
        });
        let loaded = vec![one, one];
        assert_eq!(step(&loaded, &None, &vec![one, one]).0, Step::Idle);
        assert_eq!(step(&loaded, &None, &vec![one, two]).0, Step::Wait);
    }

    /// ビルド前から見ていたファイルはビルド前の目印、増えたファイルはビルド後の目印を使う。
    #[test]
    fn merged_snapshot_prefers_the_stamps_from_before_the_build() {
        let s = |len| {
            Some(FileStamp {
                modified: None,
                len,
            })
        };
        let before_paths = vec![PathBuf::from("main.typ")];
        let before = vec![s(1)];
        let new_paths = vec![PathBuf::from("lib.typ"), PathBuf::from("main.typ")];
        let after = vec![s(7), s(2)]; // main.typ はビルド中に書き換わって2になっていた
        assert_eq!(
            merged_snapshot(&before_paths, &before, &new_paths, &after),
            vec![s(7), s(1)]
        );
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
