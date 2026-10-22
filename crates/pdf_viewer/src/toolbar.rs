//! ブラウザのPDFビューア風のツールバー(ページ移動・ページ番号入力・ズーム)。

use gpui::{
    ClickEvent, Context, KeyDownEvent, MouseButton, SharedString, Stateful, Window, div,
    prelude::*, px, rgb,
};

use crate::{
    actions::{
        NextPage, PreviousPage, RotateClockwise, RunOcr, ToggleDarkMode, ToggleFullscreen,
        ToggleSidebar, ZoomIn, ZoomOut,
    },
    page_input::PageInput,
    view::{PdfView, Tool},
    zoom::ZoomMode,
};

pub const TOOLBAR_HEIGHT: f32 = 36.0;

const BACKGROUND: u32 = 0x252525;
const TEXT: u32 = 0xdddddd;
const HOVER: u32 = 0x3a3a3a;
const ACTIVE: u32 = 0x4a4a4a;
const INPUT_BACKGROUND: u32 = 0x1a1a1a;
const BORDER: u32 = 0x555555;
const FOCUS_BORDER: u32 = 0x4d9bff;

fn button(id: &'static str, label: impl Into<SharedString>, active: bool) -> Stateful<gpui::Div> {
    div()
        .id(id)
        .h(px(24.0))
        .min_w(px(28.0))
        .px(px(6.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded_sm()
        .cursor_pointer()
        .when(active, |d| d.bg(rgb(ACTIVE)))
        .hover(|d| d.bg(rgb(HOVER)))
        .child(label.into())
}

impl PdfView {
    /// ツールバーを描画する。`current`は現在のページ(0始まり)。
    pub(crate) fn render_toolbar(
        &self,
        current: usize,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let page_count = self.page_sizes.len();
        let editing = self.page_input.as_ref();
        let page_text = match editing {
            Some(input) => format!("{}|", input.text()),
            None => (current + 1).to_string(),
        };
        let page_box = div()
            .id("page-input")
            .track_focus(&self.input_focus)
            .on_key_down(cx.listener(Self::on_input_key))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| this.start_page_input(window, cx)),
            )
            .w(px(56.0))
            .h(px(24.0))
            .px(px(6.0))
            .flex()
            .items_center()
            .justify_end()
            .rounded_sm()
            .border_1()
            .border_color(rgb(if editing.is_some() {
                FOCUS_BORDER
            } else {
                BORDER
            }))
            .bg(rgb(INPUT_BACKGROUND))
            .cursor_text()
            .child(page_text);

        let zoom_label = format!("{:.0}%", self.zoom * 100.0);
        let mode = self.zoom_mode;
        let dark = self.appearance.invert;
        let sidebar_open = self.sidebar_open;
        let tool = self.tool;

        div()
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .gap_1()
            .w_full()
            .h(px(TOOLBAR_HEIGHT))
            .bg(rgb(BACKGROUND))
            .text_color(rgb(TEXT))
            .text_sm()
            .child(
                button("sidebar", "Side", sidebar_open).on_click(cx.listener(
                    |this, _: &ClickEvent, window, cx| {
                        this.toggle_sidebar(&ToggleSidebar, window, cx)
                    },
                )),
            )
            .child(div().w(px(16.0)))
            .child(
                button("tool-hand", "Hand", tool == Tool::Hand).on_click(
                    cx.listener(|this, _: &ClickEvent, _, cx| this.set_tool(Tool::Hand, cx)),
                ),
            )
            .child(
                button("tool-select", "Select", tool == Tool::Select).on_click(
                    cx.listener(|this, _: &ClickEvent, _, cx| this.set_tool(Tool::Select, cx)),
                ),
            )
            .child(div().w(px(16.0)))
            .child(button("prev-page", "‹", false).on_click(cx.listener(
                |this, _: &ClickEvent, window, cx| this.previous_page(&PreviousPage, window, cx),
            )))
            .child(page_box)
            .child(format!("/ {page_count}"))
            .child(button("next-page", "›", false).on_click(cx.listener(
                |this, _: &ClickEvent, window, cx| this.next_page(&NextPage, window, cx),
            )))
            .child(div().w(px(16.0)))
            .child(button("zoom-out", "−", false).on_click(
                cx.listener(|this, _: &ClickEvent, window, cx| this.zoom_out(&ZoomOut, window, cx)),
            ))
            .child(div().w(px(48.0)).flex().justify_center().child(zoom_label))
            .child(button("zoom-in", "+", false).on_click(
                cx.listener(|this, _: &ClickEvent, window, cx| this.zoom_in(&ZoomIn, window, cx)),
            ))
            .child(div().w(px(16.0)))
            .child(
                button("fit-width", "Width", mode == ZoomMode::FitWidth).on_click(cx.listener(
                    |this, _: &ClickEvent, _, cx| this.set_zoom_mode(ZoomMode::FitWidth, cx),
                )),
            )
            .child(
                button("fit-page", "Page", mode == ZoomMode::FitPage).on_click(cx.listener(
                    |this, _: &ClickEvent, _, cx| this.set_zoom_mode(ZoomMode::FitPage, cx),
                )),
            )
            .child(div().w(px(16.0)))
            .child(button("ocr", "OCR", self.ocr_running).on_click(
                cx.listener(|this, _: &ClickEvent, window, cx| this.run_ocr(&RunOcr, window, cx)),
            ))
            .child(button("rotate", "Rotate", false).on_click(cx.listener(
                |this, _: &ClickEvent, window, cx| {
                    this.rotate_clockwise(&RotateClockwise, window, cx)
                },
            )))
            .child(button("dark", "Dark", dark).on_click(cx.listener(
                |this, _: &ClickEvent, window, cx| {
                    this.toggle_dark_mode(&ToggleDarkMode, window, cx)
                },
            )))
            .child(button("fullscreen", "Full", false).on_click(cx.listener(
                |this, _: &ClickEvent, window, cx| {
                    this.toggle_fullscreen(&ToggleFullscreen, window, cx)
                },
            )))
    }
}

impl PdfView {
    /// ページ番号入力欄の編集を始める。
    pub(crate) fn start_page_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (w, h) = self.viewport_size(window);
        let current = self.current_page(&self.layout(w), h);
        self.page_input = Some(PageInput::new(current + 1));
        window.focus(&self.input_focus, cx);
        cx.notify();
    }

    /// 入力欄の編集を終えて、ビュー本体にフォーカスを戻す。
    fn finish_page_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.page_input = None;
        window.focus(&self.focus_handle, cx);
        cx.notify();
    }

    pub(crate) fn on_input_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(input) = self.page_input.as_mut() else {
            return;
        };
        let keystroke = &event.keystroke;
        match keystroke.key.as_str() {
            "enter" => {
                if let Some(page) = input.commit(self.page_sizes.len()) {
                    self.scroll_to_page(page, window, cx);
                }
                self.finish_page_input(window, cx);
            }
            "escape" => self.finish_page_input(window, cx),
            "backspace" => input.backspace(),
            _ => {
                let typed = keystroke.key_char.as_deref().and_then(|s| s.chars().next());
                if let (Some(c), false) = (typed, keystroke.modifiers.control) {
                    input.push(c);
                }
            }
        }
        cx.notify();
        cx.stop_propagation();
    }
}
