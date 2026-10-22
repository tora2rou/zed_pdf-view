use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use gpui::{
    AppContext as _, Context, Decorations, Entity, ExternalPaths, FocusHandle, IntoElement,
    KeyBinding, PathPromptOptions, Render, SharedString, Window, actions, div, prelude::*, px, rgb,
};
use pdf_core::{PdfDocument, PdfError};
use pdf_viewer::PdfView;

use crate::{recent::RecentFiles, resize::resize_handles};

actions!(
    pdf_view_app,
    [
        /// ファイル選択ダイアログを開く。
        OpenFile,
        /// 最近開いたファイルの一覧を開閉する。
        ToggleRecent,
        /// 表示中のタブを閉じる。
        CloseTab,
        /// 次のタブへ移る。最後のタブからは先頭へ戻る。
        NextTab,
        /// 前のタブへ移る。先頭のタブからは最後へ戻る。
        PreviousTab,
    ]
);

/// `AppWindow`のキーコンテキスト名。
const KEY_CONTEXT: &str = "PdfViewApp";

/// アプリ固有のキーバインドを登録する。
pub fn init(cx: &mut gpui::App) {
    cx.bind_keys([
        KeyBinding::new("ctrl-o", OpenFile, Some(KEY_CONTEXT)),
        KeyBinding::new("cmd-o", OpenFile, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-shift-o", ToggleRecent, Some(KEY_CONTEXT)),
        KeyBinding::new("cmd-shift-o", ToggleRecent, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-w", CloseTab, Some(KEY_CONTEXT)),
        KeyBinding::new("cmd-w", CloseTab, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-tab", NextTab, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-shift-tab", PreviousTab, Some(KEY_CONTEXT)),
    ]);
}

const BACKGROUND: u32 = 0x252525;
const VIEWER_BACKGROUND: u32 = 0x3c3c3c;
const BORDER: u32 = 0x555555;
const TEXT: u32 = 0xdddddd;
const MUTED_TEXT: u32 = 0x999999;
const ERROR_BACKGROUND: u32 = 0x5a1e1e;

/// ウィンドウのタイトル(ファイル未選択のとき)。
pub(crate) const APP_TITLE: &str = "PDF Viewer";

/// アプリのウィンドウ全体。自前のタイトルバーとリサイズ領域でPdfViewを包む。
/// PDFを開く前(引数なしで起動したとき)は、案内を表示する。
pub struct AppWindow {
    pub(crate) title: SharedString,
    /// 開いているPDF。開いた順に並ぶ。
    pub(crate) tabs: Vec<Tab>,
    /// 表示中のタブの番号。`tabs`が空でなければ範囲内。
    pub(crate) active: usize,
    /// 直近に開こうとして失敗した理由。開くのに成功したら消える。
    pub(crate) error: Option<SharedString>,
    pub(crate) recent: RecentFiles,
    /// 履歴の保存先。`None`なら保存しない。
    recent_file: Option<PathBuf>,
    /// 最近のファイルの一覧を重ねて表示しているか。
    pub(crate) show_recent: bool,
    /// タイトルバーのドラッグ開始待ち。最初のマウス移動でウィンドウ移動に切り替える。
    pub(crate) should_move: bool,
    /// PDFを開く前など、ビューがフォーカスを持たないときにキー操作(Ctrl+Oなど)を受けるためのもの。
    pub(crate) focus_handle: FocusHandle,
}

/// 開いている1つのPDF。ページ位置やズームはタブごとに持つ。
pub(crate) struct Tab {
    pub(crate) path: PathBuf,
    pub(crate) viewer: Entity<PdfView>,
}

/// ファイル名だけを取り出す。取れなければパス全体。
pub(crate) fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}

impl AppWindow {
    /// `initial`があれば、それを開いた状態で始める。履歴は`recent_file`から読み、そこへ保存する。
    pub fn new(
        initial: Option<(PathBuf, Arc<PdfDocument>)>,
        recent_file: Option<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let recent = recent_file
            .as_deref()
            .map(RecentFiles::load)
            .unwrap_or_default();
        let mut this = Self {
            title: APP_TITLE.into(),
            tabs: Vec::new(),
            active: 0,
            error: None,
            recent,
            recent_file,
            show_recent: false,
            should_move: false,
            focus_handle: cx.focus_handle(),
        };
        match initial {
            Some((path, document)) => this.show_document(path, document, window, cx),
            None => window.focus(&this.focus_handle, cx),
        }
        this
    }

    /// 読み込み済みの文書を表示し、タイトルと最近のファイルを更新する。
    pub(crate) fn show_document(
        &mut self,
        path: PathBuf,
        document: Arc<PdfDocument>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let index = match self.tabs.iter().position(|tab| tab.path == path) {
            Some(index) => index,
            None => {
                let viewer = cx.new(|cx| PdfView::new(document, cx));
                self.tabs.push(Tab {
                    path: path.clone(),
                    viewer,
                });
                self.tabs.len() - 1
            }
        };
        self.activate_tab(index, window, cx);
        self.error = None;
        self.show_recent = false;
        self.recent.add(path);
        if let Some(file) = &self.recent_file
            && let Err(e) = self.recent.save(file)
        {
            eprintln!("最近のファイルを保存できません: {e}");
        }
        cx.notify();
    }

    /// `path`のPDFをバックグラウンドで読み込んで表示する。失敗したらエラーを表示する。
    pub(crate) fn open_path(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        cx.spawn_in(window, async move |this, cx| {
            let target = path.clone();
            let loaded: Result<PdfDocument, PdfError> = cx
                .background_spawn(async move { PdfDocument::open(&target) })
                .await;
            this.update_in(cx, |this, window, cx| match loaded {
                Ok(document) => this.show_document(path, Arc::new(document), window, cx),
                Err(e) => {
                    this.error = Some(format!("{}: {e}", file_name(&path)).into());
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// ファイル選択ダイアログを出して、選ばれたPDFを開く。
    pub(crate) fn open_file(&mut self, _: &OpenFile, window: &mut Window, cx: &mut Context<Self>) {
        let selected = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Open PDF".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            match selected.await {
                Ok(Ok(Some(paths))) => {
                    if let Some(path) = paths.into_iter().next() {
                        this.update_in(cx, |this, window, cx| this.open_path(path, window, cx))
                            .ok();
                    }
                }
                // キャンセルされた。
                Ok(Ok(None)) | Err(_) => {}
                // ダイアログを出せなかった(Linuxでxdg-desktop-portalが無い場合など)。
                Ok(Err(e)) => {
                    this.update(cx, |this, cx| {
                        this.error = Some(
                            format!(
                                "Could not open the file dialog: {e}\n\
                                 Drop a PDF onto the window, or use Recent instead."
                            )
                            .into(),
                        );
                        cx.notify();
                    })
                    .ok();
                }
            }
        })
        .detach();
    }

    pub(crate) fn toggle_recent(
        &mut self,
        _: &ToggleRecent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.show_recent = !self.show_recent;
        cx.notify();
    }

    /// ウィンドウにドロップされたファイルのうち、先頭のものを開く。
    fn on_drop(&mut self, paths: &ExternalPaths, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(path) = paths.paths().first() {
            self.open_path(path.clone(), window, cx);
        }
    }

    /// PDFを開く前の案内。
    fn render_empty_state(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_2()
            .bg(rgb(VIEWER_BACKGROUND))
            .text_color(rgb(MUTED_TEXT))
            .child("Drop a PDF here, or press Ctrl+O")
            .child(
                div()
                    .mt(px(16.0))
                    .w(px(460.0))
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(BORDER))
                    .bg(rgb(BACKGROUND))
                    .text_color(rgb(TEXT))
                    .child(
                        div()
                            .px(px(12.0))
                            .pt(px(8.0))
                            .text_xs()
                            .child("Recent files"),
                    )
                    .child(self.render_recent_list("recent-inline", cx)),
            )
    }
}

impl Render for AppWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // OS側が枠を描いてくれる環境(サーバーサイド装飾)では、二重にならないよう何も足さない。
        // 全画面中は、タイトルバーもリサイズ領域も要らない。
        let client_side = matches!(window.window_decorations(), Decorations::Client { .. })
            && !window.is_fullscreen();
        let maximized = window.is_maximized();

        let content = match self.tabs.get(self.active) {
            Some(tab) => div()
                .size_full()
                .child(tab.viewer.clone())
                .into_any_element(),
            None => self.render_empty_state(cx).into_any_element(),
        };

        div()
            .key_context(KEY_CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::open_file))
            .on_action(cx.listener(Self::toggle_recent))
            .on_action(cx.listener(Self::close_active_tab))
            .on_action(cx.listener(Self::next_tab))
            .on_action(cx.listener(Self::previous_tab))
            .on_drop(cx.listener(Self::on_drop))
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(BACKGROUND))
            .text_color(rgb(TEXT))
            .when(client_side && !maximized, |d| {
                d.border_1().border_color(rgb(BORDER))
            })
            .when(client_side, |d| {
                d.child(self.render_titlebar(maximized, cx))
            })
            .when_some(self.error.clone(), |d, message| {
                d.child(
                    div()
                        .flex_none()
                        .w_full()
                        .px(px(12.0))
                        .py(px(6.0))
                        .bg(rgb(ERROR_BACKGROUND))
                        .text_sm()
                        .child(message),
                )
            })
            .children(self.render_tab_bar(cx))
            .child(div().flex_1().min_h_0().w_full().child(content))
            .when(client_side && !maximized, |d| d.children(resize_handles()))
            .children(self.render_recent_overlay(window, cx))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Modifiers, TestAppContext, point};

    const FIXTURE: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../pdf_core/tests/fixtures/two_pages.pdf"
    );

    /// 履歴ファイルにFIXTUREだけを書いた一時ディレクトリ内のパスを返す。
    fn recent_file_with_fixture(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pdf_view_app_{name}_{}", std::process::id()));
        let file = dir.join("recent.txt");
        let mut recent = RecentFiles::default();
        recent.add(PathBuf::from(FIXTURE));
        recent.save(&file).unwrap();
        file
    }

    #[gpui::test]
    fn clicking_a_recent_entry_opens_it(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(|window, cx| {
            AppWindow::new(None, Some(recent_file_with_fixture("click")), window, cx)
        });
        view.update_in(cx, |this, _, cx| {
            this.show_recent = true;
            cx.notify();
        });
        cx.run_until_parked();
        assert!(view.read_with(cx, |this, _| this.tabs.is_empty()));

        // 最初の項目(オーバーレイの先頭)をクリックする。
        cx.simulate_click(point(px(100.0), px(50.0)), Modifiers::none());
        cx.run_until_parked();

        let (opened, shown, error) = view.read_with(cx, |this, _| {
            (!this.tabs.is_empty(), this.show_recent, this.error.clone())
        });
        assert!(
            opened,
            "viewer is still None (show_recent={shown}, error={error:?})"
        );
        assert!(!shown, "the overlay should close after opening");
    }

    /// 案内内の一覧(オーバーレイを出していない状態)の項目も、クリックで開ける。
    #[gpui::test]
    fn clicking_a_recent_entry_in_the_empty_state_opens_it(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(|window, cx| {
            AppWindow::new(None, Some(recent_file_with_fixture("inline")), window, cx)
        });
        cx.run_until_parked();

        // 案内は縦横中央に並ぶので、画面中央付近の項目を狙う。
        let size = cx.update(|window, _| window.viewport_size());
        for y in (0..f32::from(size.height) as i32).step_by(8) {
            cx.simulate_click(
                point(size.width / 2.0 - px(150.0), px(y as f32)),
                Modifiers::none(),
            );
            cx.run_until_parked();
            if view.read_with(cx, |this, _| !this.tabs.is_empty()) {
                return;
            }
        }
        panic!("no click in the empty state opened the recent file");
    }

    fn init_keybindings(cx: &mut TestAppContext) {
        cx.update(|cx| {
            pdf_viewer::init(cx);
            init(cx);
        });
    }

    /// PDFを開く前(何もフォーカスされていない状態)でも、Ctrl+Oでダイアログが出る。
    #[gpui::test]
    fn ctrl_o_opens_the_dialog_in_the_empty_state(cx: &mut TestAppContext) {
        init_keybindings(cx);
        let (_view, cx) = cx.add_window_view(|window, cx| AppWindow::new(None, None, window, cx));
        cx.run_until_parked();

        cx.simulate_keystrokes("ctrl-o");
        assert!(cx.did_prompt_for_paths(), "Ctrl+O did not open the dialog");
    }

    /// PDFを開いた後(ビューにフォーカスがある状態)でも、Ctrl+Oでダイアログが出る。
    #[gpui::test]
    fn ctrl_o_opens_the_dialog_while_viewing(cx: &mut TestAppContext) {
        init_keybindings(cx);
        let document = Arc::new(PdfDocument::open(FIXTURE).unwrap());
        let (_view, cx) = cx.add_window_view(|window, cx| {
            AppWindow::new(Some((PathBuf::from(FIXTURE), document)), None, window, cx)
        });
        cx.run_until_parked();

        cx.simulate_keystrokes("ctrl-o");
        assert!(cx.did_prompt_for_paths(), "Ctrl+O did not open the dialog");
    }

    /// Ctrl+Shift+Oで最近のファイルの一覧が開閉する(空の状態でも、閲覧中でも)。
    #[gpui::test]
    fn ctrl_shift_o_toggles_the_recent_list(cx: &mut TestAppContext) {
        init_keybindings(cx);
        let (view, cx) = cx.add_window_view(|window, cx| AppWindow::new(None, None, window, cx));
        cx.run_until_parked();

        cx.simulate_keystrokes("ctrl-shift-o");
        assert!(
            view.read_with(cx, |this, _| this.show_recent),
            "did not open"
        );
        cx.simulate_keystrokes("ctrl-shift-o");
        assert!(
            view.read_with(cx, |this, _| !this.show_recent),
            "did not close"
        );
    }

    const OTHER_FIXTURE: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../pdf_core/tests/fixtures/with_text.pdf"
    );

    fn show(view: &Entity<AppWindow>, path: &str, cx: &mut gpui::VisualTestContext) {
        let document = Arc::new(PdfDocument::open(path).unwrap());
        view.update_in(cx, |this, window, cx| {
            this.show_document(PathBuf::from(path), document, window, cx)
        });
    }

    /// 別のPDFは新しいタブで開き、そのタブを表示する。開いていたタブは残る。
    #[gpui::test]
    fn opening_another_pdf_adds_a_tab(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(|window, cx| AppWindow::new(None, None, window, cx));
        show(&view, FIXTURE, cx);
        show(&view, OTHER_FIXTURE, cx);

        let (tabs, active, title) = view.read_with(cx, |this, _| {
            (this.tabs.len(), this.active, this.title.clone())
        });
        assert_eq!((tabs, active), (2, 1));
        assert_eq!(title.as_ref(), "with_text.pdf");
    }

    /// すでに開いているPDFを開くと、新しいタブを作らずそのタブへ切り替える。
    #[gpui::test]
    fn opening_an_open_pdf_switches_to_its_tab(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(|window, cx| AppWindow::new(None, None, window, cx));
        show(&view, FIXTURE, cx);
        show(&view, OTHER_FIXTURE, cx);
        show(&view, FIXTURE, cx);

        let (tabs, active) = view.read_with(cx, |this, _| (this.tabs.len(), this.active));
        assert_eq!((tabs, active), (2, 0));
    }
}
