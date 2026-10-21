//! ページ上のリンクのホバー表示とクリックによる移動。

use gpui::{Context, MouseUpEvent, Pixels, Point, Window};
use pdf_core::LinkTarget;

use crate::{
    link_hit::{is_click, link_at, openable_uri},
    view::PdfView,
};

impl PdfView {
    /// ウィンドウ上の位置`position`にあるリンクの移動先。リンクが無ければ`None`。
    pub(crate) fn link_target_at(
        &self,
        position: Point<Pixels>,
        window: &Window,
    ) -> Option<LinkTarget> {
        let bounds = self.scroll_handle.bounds();
        // ページ領域の外(ツールバーやサイドバー)は対象外。
        if !bounds.contains(&position) {
            return None;
        }
        let (viewport_width, _) = self.viewport_size(window);
        let (scroll_x, scroll_y) = self.scroll_position();
        let x = scroll_x + f32::from(position.x - bounds.origin.x);
        let y = scroll_y + f32::from(position.y - bounds.origin.y);

        let (page, local_x, local_y) = self.layout(viewport_width).hit_page(x, y)?;
        // 画面上のpx → 回転後の画像上のポイント → 回転前の画像上のポイント(リンクの座標系)
        let shown = (local_x / self.zoom, local_y / self.zoom);
        let base = *self.base_sizes.get(page)?;
        let (bx, by) = self.appearance.rotation.to_base(base, shown);
        let links = self.document.page_links(page).ok()?;
        link_at(links, bx, by).map(|link| link.target.clone())
    }

    /// マウスの移動に合わせて、リンク上かどうか(カーソルの形)を更新する。ドラッグ中は何もしない。
    pub(crate) fn update_link_hover(
        &mut self,
        position: Point<Pixels>,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if self.drag.is_some() {
            return;
        }
        let hovering = self.link_target_at(position, window).is_some();
        if hovering != self.hovering_link {
            self.hovering_link = hovering;
            cx.notify();
        }
    }

    /// マウスのボタンを離した。ほとんど動いていなければクリックとみなし、リンク上なら移動する。
    pub(crate) fn finish_press(
        &mut self,
        event: &MouseUpEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(drag) = self.drag.take() else {
            return;
        };
        cx.notify();
        let up = (f32::from(event.position.x), f32::from(event.position.y));
        if is_click(drag.mouse, up)
            && let Some(target) = self.link_target_at(event.position, window)
        {
            self.activate_link(target, window, cx);
        }
    }

    fn activate_link(&mut self, target: LinkTarget, window: &mut Window, cx: &mut Context<Self>) {
        match target {
            LinkTarget::Page(index) => self.scroll_to_page(index, window, cx),
            // PDFは信頼できない入力なので、安全なスキームだけをブラウザ等に渡す。
            LinkTarget::Uri(uri) => {
                if let Some(uri) = openable_uri(&uri) {
                    cx.open_url(uri);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::RotateClockwise;
    use gpui::{Modifiers, MouseButton, TestAppContext, VisualTestContext, point, px};
    use pdf_core::PdfDocument;
    use std::sync::Arc;

    const WITH_LINKS: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../pdf_core/tests/fixtures/with_links.pdf"
    );

    // with_links.pdf の1ページ目のリンク(順番は pdf_core のテストと同じ)
    const TO_PAGE_2: usize = 0;
    const WEB: usize = 1;
    const JAVASCRIPT: usize = 2;
    const MAIL: usize = 4;

    fn open_view<'a>(
        cx: &'a mut TestAppContext,
        zoom: f32,
    ) -> (gpui::Entity<PdfView>, &'a mut VisualTestContext) {
        let document = Arc::new(PdfDocument::open(WITH_LINKS).unwrap());
        let (view, cx) = cx.add_window_view(|_, cx| PdfView::new(document, cx));
        if zoom != 1.0 {
            view.update_in(cx, |this, _, cx| this.set_zoom(zoom, (0.0, 0.0), cx));
        }
        cx.run_until_parked();
        (view, cx)
    }

    /// 1ページ目のリンク`link`の中心の、ウィンドウ上の位置(現在の回転・ズーム・スクロールを反映)。
    fn link_center(
        view: &gpui::Entity<PdfView>,
        cx: &mut VisualTestContext,
        link: usize,
    ) -> gpui::Point<gpui::Pixels> {
        view.update_in(cx, |this, window, _| {
            let rect = this.document.page_links(0).unwrap()[link].rect;
            let base = this.base_sizes[0];
            let center = (rect.x + rect.width / 2.0, rect.y + rect.height / 2.0);
            let shown = this.appearance.rotation.rotate_point(base, center);
            let (viewport_width, _) = this.viewport_size(window);
            let page = this.layout(viewport_width).pages[0];
            let (scroll_x, scroll_y) = this.scroll_position();
            let bounds = this.scroll_handle.bounds();
            point(
                bounds.origin.x + px(page.left + shown.0 * this.zoom - scroll_x),
                bounds.origin.y + px(page.top + shown.1 * this.zoom - scroll_y),
            )
        })
    }

    #[gpui::test]
    fn clicking_a_web_link_opens_it(cx: &mut TestAppContext) {
        let (view, cx) = open_view(cx, 1.0);
        let at = link_center(&view, cx, WEB);
        cx.simulate_click(at, Modifiers::none());
        assert_eq!(cx.opened_url().as_deref(), Some("https://example.com/a"));
    }

    #[gpui::test]
    fn clicking_a_mailto_link_opens_it(cx: &mut TestAppContext) {
        let (view, cx) = open_view(cx, 1.0);
        let at = link_center(&view, cx, MAIL);
        cx.simulate_click(at, Modifiers::none());
        assert_eq!(cx.opened_url().as_deref(), Some("mailto:me@example.com"));
    }

    /// `javascript:`のような危険なスキームは、リンク上をクリックしても開かない。
    #[gpui::test]
    fn dangerous_links_are_not_opened(cx: &mut TestAppContext) {
        let (view, cx) = open_view(cx, 1.0);
        let at = link_center(&view, cx, JAVASCRIPT);
        cx.simulate_click(at, Modifiers::none());
        assert_eq!(cx.opened_url(), None);
    }

    /// ページ内リンクをクリックすると、そのページへスクロールする。
    #[gpui::test]
    fn clicking_a_page_link_scrolls_to_the_page(cx: &mut TestAppContext) {
        // 拡大しないと全体が画面に収まり、スクロールできない
        let (view, cx) = open_view(cx, 5.0);
        let at = link_center(&view, cx, TO_PAGE_2);
        cx.simulate_click(at, Modifiers::none());
        cx.run_until_parked();
        let offset = view.read_with(cx, |this, _| f32::from(this.scroll_handle.offset().y));
        assert!(offset < 0.0, "did not scroll (offset y = {offset})");
    }

    /// リンク上で押してから大きく動かして離したら、ドラッグであってクリックではない。
    #[gpui::test]
    fn dragging_from_a_link_does_not_follow_it(cx: &mut TestAppContext) {
        let (view, cx) = open_view(cx, 1.0);
        let at = link_center(&view, cx, WEB);
        cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
        cx.simulate_mouse_move(
            point(at.x + px(40.0), at.y),
            MouseButton::Left,
            Modifiers::none(),
        );
        cx.simulate_mouse_up(
            point(at.x + px(40.0), at.y),
            MouseButton::Left,
            Modifiers::none(),
        );
        assert_eq!(cx.opened_url(), None);
    }

    /// リンクの外のクリックでは何も起きない。
    #[gpui::test]
    fn clicking_outside_links_does_nothing(cx: &mut TestAppContext) {
        let (view, cx) = open_view(cx, 1.0);
        let at = link_center(&view, cx, WEB);
        cx.simulate_click(point(at.x, at.y + px(400.0)), Modifiers::none());
        assert_eq!(cx.opened_url(), None);
    }

    /// リンク上ではカーソルがリンク用に変わり、離れると戻る。
    #[gpui::test]
    fn hovering_a_link_is_tracked(cx: &mut TestAppContext) {
        let (view, cx) = open_view(cx, 1.0);
        let at = link_center(&view, cx, WEB);
        cx.simulate_mouse_move(at, None, Modifiers::none());
        assert!(view.read_with(cx, |this, _| this.hovering_link));
        cx.simulate_mouse_move(point(at.x, at.y + px(400.0)), None, Modifiers::none());
        assert!(!view.read_with(cx, |this, _| this.hovering_link));
    }

    /// 90度回転していても、描かれたリンクの位置でクリックできる。
    #[gpui::test]
    fn links_work_on_a_rotated_view(cx: &mut TestAppContext) {
        let (view, cx) = open_view(cx, 1.0);
        view.update_in(cx, |this, window, cx| {
            this.rotate_clockwise(&RotateClockwise, window, cx)
        });
        cx.run_until_parked();
        let at = link_center(&view, cx, WEB);
        cx.simulate_click(at, Modifiers::none());
        assert_eq!(cx.opened_url().as_deref(), Some("https://example.com/a"));
    }
}
