use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use gpui::{
    AppContext as _, Context, Decorations, Entity, ExternalPaths, Focusable as _, IntoElement,
    KeyBinding, PathPromptOptions, Render, SharedString, Window, actions, div, prelude::*, px, rgb,
};
use pdf_core::{PdfDocument, PdfError};
use pdf_viewer::PdfView;

use crate::{
    recent::{RecentFiles, recent_files_path},
    resize::resize_handles,
};

actions!(
    pdf_view_app,
    [
        /// ファイル選択ダイアログを開く。
        OpenFile,
        /// 最近開いたファイルの一覧を開閉する。
        ToggleRecent,
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
    ]);
}

const BACKGROUND: u32 = 0x252525;
const VIEWER_BACKGROUND: u32 = 0x3c3c3c;
const BORDER: u32 = 0x555555;
const TEXT: u32 = 0xdddddd;
const MUTED_TEXT: u32 = 0x999999;
const ERROR_BACKGROUND: u32 = 0x5a1e1e;

/// ウィンドウのタイトル(ファイル未選択のとき)。
const APP_TITLE: &str = "PDF Viewer";

/// アプリのウィンドウ全体。自前のタイトルバーとリサイズ領域でPdfViewを包む。
/// PDFを開く前(引数なしで起動したとき)は、案内を表示する。
pub struct AppWindow {
    pub(crate) title: SharedString,
    pub(crate) viewer: Option<Entity<PdfView>>,
    /// 直近に開こうとして失敗した理由。開くのに成功したら消える。
    pub(crate) error: Option<SharedString>,
    pub(crate) recent: RecentFiles,
    /// 最近のファイルの一覧を重ねて表示しているか。
    pub(crate) show_recent: bool,
    /// タイトルバーのドラッグ開始待ち。最初のマウス移動でウィンドウ移動に切り替える。
    pub(crate) should_move: bool,
}

/// ファイル名だけを取り出す。取れなければパス全体。
fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}

impl AppWindow {
    /// `initial`があれば、それを開いた状態で始める。
    pub fn new(
        initial: Option<(PathBuf, Arc<PdfDocument>)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let recent = recent_files_path()
            .map(|file| RecentFiles::load(&file))
            .unwrap_or_default();
        let mut this = Self {
            title: APP_TITLE.into(),
            viewer: None,
            error: None,
            recent,
            show_recent: false,
            should_move: false,
        };
        if let Some((path, document)) = initial {
            this.show_document(path, document, window, cx);
        }
        this
    }

    /// 読み込み済みの文書を表示し、タイトルと最近のファイルを更新する。
    fn show_document(
        &mut self,
        path: PathBuf,
        document: Arc<PdfDocument>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match &self.viewer {
            Some(viewer) => viewer.update(cx, |view, cx| view.set_document(document, window, cx)),
            None => {
                let viewer = cx.new(|cx| PdfView::new(document, cx));
                self.viewer = Some(viewer);
            }
        }
        if let Some(viewer) = &self.viewer {
            // キーボード操作(ページ移動・ズーム)を受けられるよう、フォーカスを渡す。
            window.focus(&viewer.focus_handle(cx), cx);
        }
        self.title = file_name(&path).into();
        window.set_window_title(&self.title);
        self.error = None;
        self.show_recent = false;
        self.recent.add(path);
        if let Some(file) = recent_files_path()
            && let Err(e) = self.recent.save(&file)
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
            // キャンセルされた場合と、ダイアログを出せなかった場合は何もしない。
            if let Ok(Ok(Some(paths))) = selected.await
                && let Some(path) = paths.into_iter().next()
            {
                this.update_in(cx, |this, window, cx| this.open_path(path, window, cx))
                    .ok();
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
                    .child(self.render_recent_list(cx)),
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

        let content = match &self.viewer {
            Some(viewer) => div().size_full().child(viewer.clone()).into_any_element(),
            None => self.render_empty_state(cx).into_any_element(),
        };

        div()
            .key_context(KEY_CONTEXT)
            .on_action(cx.listener(Self::open_file))
            .on_action(cx.listener(Self::toggle_recent))
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
            .child(div().flex_1().min_h_0().w_full().child(content))
            .when(client_side && !maximized, |d| d.children(resize_handles()))
            .children(self.render_recent_overlay(window, cx))
    }
}
