//! 自前のタイトルバー(ファイル名・ドラッグ移動・最小化/最大化/閉じるボタン)。

use gpui::{
    ClickEvent, Context, IntoElement, MouseButton, Stateful, Window, WindowControlArea, div,
    prelude::*, px, rgb,
};

use crate::app_window::{AppWindow, OpenFile, ToggleRecent};

pub const TITLEBAR_HEIGHT: f32 = 32.0;

const BACKGROUND: u32 = 0x1e1e1e;
const TEXT: u32 = 0xdddddd;
const HOVER: u32 = 0x3a3a3a;
const CLOSE_HOVER: u32 = 0xc42b1c;
const BUTTON_WIDTH: f32 = 46.0;

fn control_button(id: &'static str) -> Stateful<gpui::Div> {
    div()
        .id(id)
        .w(px(BUTTON_WIDTH))
        .h_full()
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        // ボタン上のマウス押下がドラッグ移動を始めないようにする。
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
}

/// タイトルバー左側の文字ボタン(Open / Recent)。
fn text_button(id: &'static str, label: &'static str) -> Stateful<gpui::Div> {
    div()
        .id(id)
        .h(px(24.0))
        .mx(px(2.0))
        .px(px(8.0))
        .flex()
        .items_center()
        .rounded_sm()
        .cursor_pointer()
        .hover(|d| d.bg(rgb(HOVER)))
        // ボタン上のマウス押下がドラッグ移動を始めないようにする。
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(label)
}

impl AppWindow {
    pub(crate) fn render_titlebar(
        &self,
        maximized: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let open = text_button("open", "Open").on_click(cx.listener(
            |this, _: &ClickEvent, window: &mut Window, cx| this.open_file(&OpenFile, window, cx),
        ));
        let recent = text_button("recent", "Recent").on_click(cx.listener(
            |this, _: &ClickEvent, window: &mut Window, cx| {
                this.toggle_recent(&ToggleRecent, window, cx)
            },
        ));
        let minimize = control_button("minimize")
            .window_control_area(WindowControlArea::Min)
            .hover(|d| d.bg(rgb(HOVER)))
            .on_click(
                cx.listener(|_, _: &ClickEvent, window: &mut Window, _| window.minimize_window()),
            )
            .child(div().w(px(10.0)).h(px(1.0)).bg(rgb(TEXT)));

        let maximize_icon = if maximized {
            // 復元アイコン: 少しずらした2枚の四角
            div()
                .relative()
                .size(px(10.0))
                .child(outlined_square(8.0).absolute().top(px(0.0)).right(px(0.0)))
                .child(
                    outlined_square(8.0)
                        .absolute()
                        .bottom(px(0.0))
                        .left(px(0.0))
                        .bg(rgb(BACKGROUND)),
                )
        } else {
            div().size(px(10.0)).child(outlined_square(10.0))
        };
        let maximize = control_button("maximize")
            .window_control_area(WindowControlArea::Max)
            .hover(|d| d.bg(rgb(HOVER)))
            .on_click(cx.listener(|_, _: &ClickEvent, window: &mut Window, _| window.zoom_window()))
            .child(maximize_icon);

        let close = control_button("close")
            .window_control_area(WindowControlArea::Close)
            .hover(|d| d.bg(rgb(CLOSE_HOVER)))
            .on_click(
                cx.listener(|_, _: &ClickEvent, window: &mut Window, _| window.remove_window()),
            )
            .child("×");

        div()
            .id("titlebar")
            .flex()
            .flex_none()
            .items_center()
            .w_full()
            .h(px(TITLEBAR_HEIGHT))
            .bg(rgb(BACKGROUND))
            .text_color(rgb(TEXT))
            .text_sm()
            .child(open)
            .child(recent)
            .child(
                // 空き部分全体がドラッグ移動の領域。ダブルクリックで最大化/復元。
                div()
                    .id("titlebar-drag")
                    .window_control_area(WindowControlArea::Drag)
                    .flex_1()
                    .h_full()
                    .px(px(12.0))
                    .flex()
                    .items_center()
                    .overflow_hidden()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, _, _| this.should_move = true),
                    )
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|this, _, _, _| this.should_move = false),
                    )
                    .on_mouse_move(cx.listener(|this, _, window: &mut Window, _| {
                        if this.should_move {
                            this.should_move = false;
                            window.start_window_move();
                        }
                    }))
                    .on_click(
                        cx.listener(|_, event: &ClickEvent, window: &mut Window, _| {
                            if event.click_count() == 2 {
                                window.zoom_window();
                            }
                        }),
                    )
                    .child(self.title.clone()),
            )
            .child(minimize)
            .child(maximize)
            .child(close)
    }
}

fn outlined_square(size: f32) -> gpui::Div {
    div().size(px(size)).border_1().border_color(rgb(TEXT))
}
