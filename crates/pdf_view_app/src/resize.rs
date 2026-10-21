//! クライアントサイド装飾のウィンドウで、縁や角をドラッグしてリサイズするための透明な領域。

use gpui::{CursorStyle, IntoElement, MouseButton, ResizeEdge, Window, div, prelude::*, px};

/// 辺の太さ。
const EDGE: f32 = 5.0;
/// 角の大きさ。辺より大きくして掴みやすくする。
const CORNER: f32 = 12.0;

/// 8方向のリサイズ領域。ウィンドウの`relative`な親に`absolute`で重ねる。
pub fn resize_handles() -> impl IntoIterator<Item = impl IntoElement> {
    [
        (ResizeEdge::Top, CursorStyle::ResizeUpDown),
        (ResizeEdge::Bottom, CursorStyle::ResizeUpDown),
        (ResizeEdge::Left, CursorStyle::ResizeLeftRight),
        (ResizeEdge::Right, CursorStyle::ResizeLeftRight),
        (ResizeEdge::TopLeft, CursorStyle::ResizeUpRightDownLeft),
        (ResizeEdge::BottomRight, CursorStyle::ResizeUpRightDownLeft),
        (ResizeEdge::TopRight, CursorStyle::ResizeUpLeftDownRight),
        (ResizeEdge::BottomLeft, CursorStyle::ResizeUpLeftDownRight),
    ]
    .map(|(edge, cursor)| handle(edge, cursor))
}

fn handle(edge: ResizeEdge, cursor: CursorStyle) -> impl IntoElement {
    let base = div()
        .id(format!("resize-{edge:?}"))
        .absolute()
        .cursor(cursor)
        .on_mouse_down(MouseButton::Left, move |_, window: &mut Window, _| {
            window.start_window_resize(edge)
        });
    match edge {
        ResizeEdge::Top => base.top_0().left(px(CORNER)).right(px(CORNER)).h(px(EDGE)),
        ResizeEdge::Bottom => base
            .bottom_0()
            .left(px(CORNER))
            .right(px(CORNER))
            .h(px(EDGE)),
        ResizeEdge::Left => base.left_0().top(px(CORNER)).bottom(px(CORNER)).w(px(EDGE)),
        ResizeEdge::Right => base
            .right_0()
            .top(px(CORNER))
            .bottom(px(CORNER))
            .w(px(EDGE)),
        ResizeEdge::TopLeft => base.top_0().left_0().size(px(CORNER)),
        ResizeEdge::TopRight => base.top_0().right_0().size(px(CORNER)),
        ResizeEdge::BottomLeft => base.bottom_0().left_0().size(px(CORNER)),
        ResizeEdge::BottomRight => base.bottom_0().right_0().size(px(CORNER)),
    }
}
