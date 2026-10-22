//! 縦のスクロールバー。`ScrollHandle`で動く領域の右端に重ねて使う。

use std::{cell::Cell, rc::Rc};

use gpui::{
    App, AppContext as _, Context, DragMoveEvent, InteractiveElement as _, IntoElement,
    MouseButton, MouseDownEvent, ParentElement as _, Render, ScrollHandle,
    StatefulInteractiveElement as _, Styled as _, Window, div, px, rgba,
};

use crate::scrollbar_math::{offset_at, offset_for_thumb_top, thumb};

/// バーの見た目の幅。
const WIDTH: f32 = 10.0;
/// クリックを受ける幅。見た目のバーの左側に広げてあり、掴み・押しやすくしてある。
const HIT_WIDTH: f32 = 20.0;
const TRACK_COLOR: u32 = 0xffffff10;
const THUMB_COLOR: u32 = 0x888888cc;
const THUMB_HOVER_COLOR: u32 = 0xaaaaaaee;

/// つまみのドラッグ中の状態。`grab`は、掴んだ位置のつまみ上端からの距離。
#[derive(Clone)]
struct ThumbDrag {
    /// 同時に複数のバーがあるので、どのバーのドラッグかを区別する。
    id: &'static str,
    grab: Rc<Cell<f32>>,
}

/// ドラッグ中にカーソルに付いてくる表示。何も出さない。
struct NoGhost;

impl Render for NoGhost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

/// `handle`が動かす領域の縦スクロールバー。スクロールする余地がなければ`None`。
/// つまみの操作で位置を変えるときは`on_scroll`を呼ぶ(引数は新しいスクロール位置。正の値 = 下へ)。
///
/// `content`は内容全体の高さ。`handle`の測定値は描画のあとで更新されるので、内容が変わった直後に
/// 古い値を使わないよう、呼び出し側が与える。見える範囲の高さは、リサイズ時に再描画されるので`handle`から読む。
pub fn vertical_scrollbar<F>(
    id: &'static str,
    handle: &ScrollHandle,
    content: f32,
    on_scroll: F,
) -> Option<impl IntoElement + use<F>>
where
    F: Fn(f32, &mut Window, &mut App) + 'static,
{
    let bounds = handle.bounds();
    let viewport = f32::from(bounds.size.height);
    let offset = -f32::from(handle.offset().y);
    let top = f32::from(bounds.origin.y);
    let thumb = thumb(viewport, viewport, content, offset)?;

    let on_scroll = Rc::new(on_scroll);
    let grab = Rc::new(Cell::new(0.0));
    let drag = ThumbDrag {
        id,
        grab: Rc::clone(&grab),
    };

    let click_scroll = Rc::clone(&on_scroll);
    let drag_scroll = on_scroll;
    Some(
        div()
            .id(id)
            .absolute()
            .top_0()
            .right_0()
            .bottom_0()
            .w(px(HIT_WIDTH))
            // トラックのクリックで、その位置へ飛ぶ。つまみの上のクリックはつまみが受ける。
            .on_mouse_down(
                MouseButton::Left,
                move |event: &MouseDownEvent, window, cx| {
                    let position = f32::from(event.position.y) - top;
                    click_scroll(offset_at(viewport, viewport, content, position), window, cx);
                    cx.stop_propagation();
                },
            )
            .on_drag_move(move |event: &DragMoveEvent<ThumbDrag>, window, cx| {
                let drag = event.drag(cx);
                if drag.id != id {
                    return;
                }
                let thumb_top = f32::from(event.event.position.y) - top - drag.grab.get();
                let offset = offset_for_thumb_top(viewport, viewport, content, thumb_top);
                drag_scroll(offset, window, cx);
            })
            .child(
                div()
                    .absolute()
                    .top_0()
                    .right_0()
                    .bottom_0()
                    .w(px(WIDTH))
                    .bg(rgba(TRACK_COLOR)),
            )
            .child(
                div()
                    .id((id, 1usize))
                    .absolute()
                    .left(px(HIT_WIDTH - WIDTH + 2.0))
                    .right(px(2.0))
                    .top(px(thumb.top))
                    .h(px(thumb.len))
                    .rounded_full()
                    .bg(rgba(THUMB_COLOR))
                    .hover(|s| s.bg(rgba(THUMB_HOVER_COLOR)))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_drag(drag, move |drag, cursor_offset, _, cx| {
                        drag.grab.set(f32::from(cursor_offset.y));
                        cx.new(|_| NoGhost)
                    }),
            ),
    )
}
