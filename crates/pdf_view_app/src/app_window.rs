use gpui::{
    Context, Decorations, Entity, IntoElement, Render, SharedString, Window, div, prelude::*, rgb,
};
use pdf_viewer::PdfView;

use crate::resize::resize_handles;

const BACKGROUND: u32 = 0x252525;
const BORDER: u32 = 0x555555;

/// アプリのウィンドウ全体。自前のタイトルバーとリサイズ領域でPdfViewを包む。
pub struct AppWindow {
    pub(crate) title: SharedString,
    viewer: Entity<PdfView>,
    /// タイトルバーのドラッグ開始待ち。最初のマウス移動でウィンドウ移動に切り替える。
    pub(crate) should_move: bool,
}

impl AppWindow {
    pub fn new(title: SharedString, viewer: Entity<PdfView>) -> Self {
        Self {
            title,
            viewer,
            should_move: false,
        }
    }
}

impl Render for AppWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // OS側が枠を描いてくれる環境(サーバーサイド装飾)では、二重にならないよう何も足さない。
        let client_side = matches!(window.window_decorations(), Decorations::Client { .. });
        let maximized = window.is_maximized();

        div()
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(BACKGROUND))
            .when(client_side && !maximized, |d| {
                d.border_1().border_color(rgb(BORDER))
            })
            .when(client_side, |d| {
                d.child(self.render_titlebar(maximized, cx))
            })
            .child(div().flex_1().min_h_0().w_full().child(self.viewer.clone()))
            .when(client_side && !maximized, |d| d.children(resize_handles()))
    }
}
