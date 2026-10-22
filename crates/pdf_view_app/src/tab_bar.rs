//! 開いているPDFのタブバー(切り替えと閉じる操作)。

use gpui::{
    ClickEvent, Context, Focusable as _, IntoElement, MouseButton, Window, div, prelude::*, px, rgb,
};

use crate::app_window::{APP_TITLE, AppWindow, file_name};

pub const TAB_BAR_HEIGHT: f32 = 30.0;

const BACKGROUND: u32 = 0x1e1e1e;
const ACTIVE_BACKGROUND: u32 = 0x3c3c3c;
const HOVER: u32 = 0x2f2f2f;
const TEXT: u32 = 0xdddddd;
const MUTED_TEXT: u32 = 0x999999;
const CLOSE_HOVER: u32 = 0x555555;
const TAB_MAX_WIDTH: f32 = 200.0;

impl AppWindow {
    /// `index`のタブを表示する。
    pub(crate) fn activate_tab(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(tab) = self.tabs.get(index) else {
            return;
        };
        self.active = index;
        self.title = file_name(&tab.path).into();
        window.set_window_title(&self.title);
        window.focus(&tab.viewer.focus_handle(cx), cx);
        cx.notify();
    }

    /// `index`のタブを閉じる。表示中のタブを閉じたら、隣のタブを表示する。
    pub(crate) fn close_tab(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if index >= self.tabs.len() {
            return;
        }
        self.tabs.remove(index);
        if self.tabs.is_empty() {
            self.active = 0;
            self.title = APP_TITLE.into();
            window.set_window_title(&self.title);
            window.focus(&self.focus_handle, cx);
            cx.notify();
            return;
        }
        // 閉じたタブより右にあった表示中のタブは、番号が1つ減る。
        let active = if index < self.active {
            self.active - 1
        } else {
            self.active.min(self.tabs.len() - 1)
        };
        self.activate_tab(active, window, cx);
    }

    /// タブバー。開いているPDFがなければ`None`。
    pub(crate) fn render_tab_bar(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        if self.tabs.is_empty() {
            return None;
        }
        let tabs = self.tabs.iter().enumerate().map(|(index, tab)| {
            let active = index == self.active;
            let close = div()
                .id(("tab-close", index))
                .flex_none()
                .size(px(18.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded_sm()
                .text_color(rgb(MUTED_TEXT))
                .hover(|d| d.bg(rgb(CLOSE_HOVER)).text_color(rgb(TEXT)))
                // 閉じるボタンの押下が、タブの切り替えにならないようにする。
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.close_tab(index, window, cx);
                    cx.stop_propagation();
                }))
                .child("×");
            div()
                .id(("tab", index))
                .flex()
                .items_center()
                .gap_1()
                .max_w(px(TAB_MAX_WIDTH))
                .h_full()
                .pl(px(10.0))
                .pr(px(6.0))
                .text_sm()
                .cursor_pointer()
                .when(active, |d| {
                    d.bg(rgb(ACTIVE_BACKGROUND)).text_color(rgb(TEXT))
                })
                .when(!active, |d| {
                    d.text_color(rgb(MUTED_TEXT))
                        .hover(|d| d.bg(rgb(HOVER)).text_color(rgb(TEXT)))
                })
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.activate_tab(index, window, cx)
                }))
                .child(
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .child(file_name(&tab.path)),
                )
                .child(close)
        });
        Some(
            div()
                .flex()
                .flex_none()
                .w_full()
                .h(px(TAB_BAR_HEIGHT))
                .bg(rgb(BACKGROUND))
                .children(tabs),
        )
    }
}

#[cfg(test)]
mod tests {
    use std::{path::PathBuf, sync::Arc};

    use gpui::{Entity, TestAppContext, VisualTestContext};
    use pdf_core::PdfDocument;

    use crate::app_window::AppWindow;

    const FIXTURES: [&str; 3] = [
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../pdf_core/tests/fixtures/two_pages.pdf"
        ),
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../pdf_core/tests/fixtures/with_text.pdf"
        ),
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../pdf_core/tests/fixtures/with_links.pdf"
        ),
    ];

    fn three_tabs(cx: &mut TestAppContext) -> (Entity<AppWindow>, &mut VisualTestContext) {
        let (view, cx) = cx.add_window_view(|window, cx| AppWindow::new(None, None, window, cx));
        for path in FIXTURES {
            let document = Arc::new(PdfDocument::open(path).unwrap());
            view.update_in(cx, |this, window, cx| {
                this.show_document(PathBuf::from(path), document, window, cx)
            });
        }
        (view, cx)
    }

    fn state(view: &Entity<AppWindow>, cx: &VisualTestContext) -> (usize, usize, String) {
        view.read_with(cx, |this, _| {
            (this.tabs.len(), this.active, this.title.to_string())
        })
    }

    #[gpui::test]
    fn selecting_a_tab_shows_it_and_retitles_the_window(cx: &mut TestAppContext) {
        let (view, cx) = three_tabs(cx);
        view.update_in(cx, |this, window, cx| this.activate_tab(0, window, cx));
        assert_eq!(state(&view, cx), (3, 0, "two_pages.pdf".into()));
    }

    /// 表示中のタブを閉じると、同じ位置に来た右隣(なければ左隣)のタブを表示する。
    #[gpui::test]
    fn closing_the_active_tab_shows_a_neighbour(cx: &mut TestAppContext) {
        let (view, cx) = three_tabs(cx);
        view.update_in(cx, |this, window, cx| this.close_tab(2, window, cx));
        assert_eq!(state(&view, cx), (2, 1, "with_text.pdf".into()));

        view.update_in(cx, |this, window, cx| {
            this.activate_tab(0, window, cx);
            this.close_tab(0, window, cx);
        });
        assert_eq!(state(&view, cx), (1, 0, "with_text.pdf".into()));
    }

    /// 表示していないタブを、表示中のタブより左で閉じても、表示は変わらない。
    #[gpui::test]
    fn closing_a_tab_to_the_left_keeps_the_active_one(cx: &mut TestAppContext) {
        let (view, cx) = three_tabs(cx);
        view.update_in(cx, |this, window, cx| this.close_tab(0, window, cx));
        assert_eq!(state(&view, cx), (2, 1, "with_links.pdf".into()));
    }

    #[gpui::test]
    fn closing_the_last_tab_returns_to_the_empty_state(cx: &mut TestAppContext) {
        let (view, cx) = three_tabs(cx);
        view.update_in(cx, |this, window, cx| {
            for _ in 0..3 {
                this.close_tab(0, window, cx);
            }
        });
        assert_eq!(state(&view, cx), (0, 0, "PDF Viewer".into()));
    }
}
