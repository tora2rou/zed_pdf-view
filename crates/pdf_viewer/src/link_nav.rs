//! ページ上のリンクのホバー表示とクリックによる移動。

use gpui::{AppContext as _, Context, MouseUpEvent, Pixels, Point, Window};
use pdf_core::LinkTarget;

use crate::{
    link_hit::{is_click, link_at, openable_uri},
    view::{PdfView, Tool},
};

impl PdfView {
    /// 見えているページのリンク(注釈と、文字の中のURL)を、バックグラウンドで先に読み込んでおく。
    /// ページの解釈が要るので、最初にマウスを重ねたときに画面を更新するスレッドで読むと、引っかかる。
    pub(crate) fn request_page_links(
        &mut self,
        visible: &std::ops::Range<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for index in visible.clone() {
            if !self.links_requested.insert(index) {
                continue;
            }
            let document = self.document.clone();
            cx.spawn_in(window, async move |_, cx| {
                // 結果は文書の側に保存される。ここでは、読み込みを済ませるだけ。
                cx.background_spawn(async move {
                    document.page_links(index).ok();
                })
                .await;
            })
            .detach();
        }
    }
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
        // まだバックグラウンドで読み込み中のページは、リンクなしとして扱う(待たない)。
        let links = self.document.page_links_if_ready(page)?;
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
        let click = is_click(drag.mouse, up);
        if self.tool == Tool::Select {
            self.finish_selection(cx);
        }
        let target = self.link_target_at(event.position, window);
        log::debug!("mouse up: click={click}, link={target:?}");
        if click && let Some(target) = target {
            self.activate_link(target, window, cx);
        }
    }

    fn activate_link(&mut self, target: LinkTarget, window: &mut Window, cx: &mut Context<Self>) {
        match target {
            LinkTarget::Page(index) => {
                log::debug!("following a link to page {}", index + 1);
                self.scroll_to_page(index, window, cx)
            }
            // PDFは信頼できない入力なので、安全なスキームだけをブラウザ等に渡す。
            LinkTarget::Uri(uri) => match openable_uri(&uri) {
                Some(uri) => {
                    log::info!("opening link: {uri}");
                    cx.open_url(uri);
                    // 外部のブラウザが開くまで少しかかることがあるので、押したことを画面にも出す。
                    self.show_notice(&format!("Opening {uri}"), window, cx);
                }
                None => {
                    log::warn!("refused to open link: {uri}");
                    self.show_notice(&format!("Blocked link: {uri}"), window, cx);
                }
            },
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

    const WITH_TEXT: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../pdf_core/tests/fixtures/with_text.pdf"
    );

    fn open_view(
        cx: &mut TestAppContext,
        zoom: f32,
    ) -> (gpui::Entity<PdfView>, &mut VisualTestContext) {
        open_view_of(WITH_LINKS, cx, zoom)
    }

    fn open_view_of<'a>(
        path: &str,
        cx: &'a mut TestAppContext,
        zoom: f32,
    ) -> (gpui::Entity<PdfView>, &'a mut VisualTestContext) {
        let document = Arc::new(PdfDocument::open(path).unwrap());
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
            let shown = this.appearance.rotation.point_from_base(base, center);
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

    /// URLを開いたら、押したことが画面にも出て、しばらくすると消える。
    #[gpui::test]
    fn opening_a_link_shows_a_notice_that_fades(cx: &mut TestAppContext) {
        let (view, cx) = open_view(cx, 1.0);
        let at = link_center(&view, cx, WEB);
        cx.simulate_click(at, Modifiers::none());

        let shown = view.read_with(cx, |this, _| this.notice.clone());
        assert_eq!(shown.as_deref(), Some("Opening https://example.com/a"));

        cx.executor()
            .advance_clock(crate::notice::NOTICE_DURATION * 2);
        cx.run_until_parked();
        assert_eq!(view.read_with(cx, |this, _| this.notice.clone()), None);
    }

    /// `javascript:`のような危険なスキームは、リンク上をクリックしても開かない。画面には、ブロックしたことを出す。
    #[gpui::test]
    fn dangerous_links_are_not_opened(cx: &mut TestAppContext) {
        let (view, cx) = open_view(cx, 1.0);
        let at = link_center(&view, cx, JAVASCRIPT);
        cx.simulate_click(at, Modifiers::none());
        assert_eq!(cx.opened_url(), None);
        let shown = view.read_with(cx, |this, _| this.notice.clone());
        assert_eq!(shown.as_deref(), Some("Blocked link: javascript:alert(1)"));
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

    /// 見えているページのリンクは、バックグラウンドで先に読み込まれる(マウスを重ねる前に準備済み)。
    #[gpui::test]
    fn visible_pages_links_are_prepared_in_the_background(cx: &mut TestAppContext) {
        let (view, cx) = open_view(cx, 1.0);
        let ready = view.read_with(cx, |this, _| {
            (0..3)
                .map(|i| this.document.page_links_if_ready(i).is_some())
                .collect::<Vec<_>>()
        });
        assert_eq!(ready, [true, true, true]);
    }

    /// 注釈の無い、文字だけのURLも、クリックで開ける(ブラウザのPDFビューアと同じ)。
    #[gpui::test]
    fn clicking_a_url_in_plain_text_opens_it(cx: &mut TestAppContext) {
        let (view, cx) = open_view_of(WITH_TEXT, cx, 1.0);
        let at = view.update_in(cx, |this, window, _| {
            let rect = this.document.page_links(0).unwrap()[0].rect;
            let (viewport_width, _) = this.viewport_size(window);
            let page = this.layout(viewport_width).pages[0];
            let bounds = this.scroll_handle.bounds();
            point(
                bounds.origin.x + px(page.left + rect.x + rect.width / 2.0),
                bounds.origin.y + px(page.top + rect.y + rect.height / 2.0),
            )
        });
        cx.simulate_click(at, Modifiers::none());
        assert_eq!(
            cx.opened_url().as_deref(),
            Some("https://example.com/a/doc.pdf")
        );
    }
}
