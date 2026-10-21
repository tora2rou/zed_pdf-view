//! 最近開いたファイルの一覧表示(空の状態の案内と、開いている間に出すオーバーレイ)。

use std::path::Path;

use gpui::{ClickEvent, Context, IntoElement, MouseButton, Window, div, prelude::*, px, rgb};

use crate::{app_window::AppWindow, titlebar::TITLEBAR_HEIGHT};

const PANEL_BACKGROUND: u32 = 0x1e1e1e;
const BORDER: u32 = 0x555555;
const HOVER: u32 = 0x3a3a3a;
const MUTED_TEXT: u32 = 0x999999;
const PANEL_WIDTH: f32 = 460.0;

/// 親ディレクトリの表示用文字列。無ければ空。
fn parent_display(path: &Path) -> String {
    path.parent()
        .map(|p| p.display().to_string())
        .unwrap_or_default()
}

impl AppWindow {
    /// 最近のファイルの一覧。クリックで開く。履歴が空なら案内を出す。
    ///
    /// `id_prefix`は項目の要素IDの接頭辞。GPUIは要素IDでクリック状態を共有するので、
    /// 同時に表示される一覧(案内とオーバーレイ)では必ず別の値にする。
    pub(crate) fn render_recent_list(
        &self,
        id_prefix: &'static str,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let mut list = div().flex().flex_col().w_full();
        if self.recent.paths().is_empty() {
            return list
                .p(px(12.0))
                .text_sm()
                .text_color(rgb(MUTED_TEXT))
                .child("No recent files");
        }
        for (index, path) in self.recent.paths().iter().enumerate() {
            let name = path.file_name().map_or_else(
                || path.display().to_string(),
                |n| n.to_string_lossy().into_owned(),
            );
            let target = path.clone();
            list = list.child(
                div()
                    .id((id_prefix, index))
                    .px(px(12.0))
                    .py(px(6.0))
                    .cursor_pointer()
                    .hover(|d| d.bg(rgb(HOVER)))
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        this.show_recent = false;
                        this.open_path(target.clone(), window, cx);
                    }))
                    .child(div().text_sm().child(name))
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(MUTED_TEXT))
                            .child(parent_display(path)),
                    ),
            );
        }
        list
    }

    /// `show_recent`のときだけ、ウィンドウ上部に一覧を重ねる。外側をクリックすると閉じる。
    pub(crate) fn render_recent_overlay(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        if !self.show_recent {
            return None;
        }
        let top = if window.is_fullscreen() {
            8.0
        } else {
            TITLEBAR_HEIGHT + 4.0
        };
        // バックドロップ(外側クリックで閉じる)とパネルは兄弟にする。パネルを子にして
        // stop_propagationで背面を止めると、パネル内の項目のクリックまで成立しなくなる。
        Some(
            div()
                .absolute()
                .inset_0()
                .child(div().absolute().inset_0().on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        this.show_recent = false;
                        cx.notify();
                    }),
                ))
                .child(
                    div()
                        .absolute()
                        .top(px(top))
                        .left(px(8.0))
                        .w(px(PANEL_WIDTH))
                        .max_h(px(420.0))
                        .overflow_hidden()
                        .rounded_md()
                        .border_1()
                        .border_color(rgb(BORDER))
                        .bg(rgb(PANEL_BACKGROUND))
                        // パネル上では背面のバックドロップにマウスが届かないようにする。
                        .occlude()
                        .child(self.render_recent_list("recent-overlay", cx)),
                ),
        )
    }
}
