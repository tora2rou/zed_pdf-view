use std::sync::Arc;

use gpui::{
    App, Context, FocusHandle, Focusable, InteractiveElement, IntoElement, ParentElement, Pixels,
    Point, Render, ScrollDelta, ScrollEvent, ScrollHandle, StatefulInteractiveElement, Styled,
    Window, div, img, point, px, rgb,
};
use pdf_core::PdfDocument;

use crate::{
    actions::{
        FirstPage, KEY_CONTEXT, LastPage, NextPage, PreviousPage, ResetZoom, ZoomIn, ZoomOut,
    },
    images::PageImages,
    layout::{self, Layout},
    zoom,
};

/// ホイールが行単位(マウス)で届いたときの1行あたりのpx。
const WHEEL_LINE_HEIGHT: f32 = 40.0;
/// ページ情報の帯の高さ。
const STATUS_BAR_HEIGHT: f32 = 24.0;
/// ページ情報の判定位置: 画面上端からこの割合だけ下にあるページを「現在のページ」とする。
const CURRENT_PAGE_PROBE: f32 = 0.3;

/// PDFを表示するビュー。全ページを縦に並べてスクロール・ズームできる。
///
/// Zed本体の`image_viewer`のようにItemとして組み込めるよう、GPUIの`Render`/`Focusable`だけに
/// 依存し、ファイルの読み込みは呼び出し側が行う。
pub struct PdfView {
    document: Arc<PdfDocument>,
    /// 各ページのPDFポイント単位のサイズ。
    page_sizes: Vec<(f32, f32)>,
    zoom: f32,
    scroll_handle: ScrollHandle,
    /// 次の描画の冒頭で適用するスクロール位置(GPUIのスクロール処理との競合を避けるため遅延させる)。
    pending_offset: Option<Point<Pixels>>,
    images: PageImages,
    focus_handle: FocusHandle,
}

impl PdfView {
    pub fn new(document: Arc<PdfDocument>, cx: &mut Context<Self>) -> Self {
        let page_sizes = (0..document.page_count())
            .map(|i| document.page_size(i).unwrap_or((0.0, 0.0)))
            .collect();
        Self {
            document,
            page_sizes,
            zoom: zoom::DEFAULT_ZOOM,
            scroll_handle: ScrollHandle::new(),
            pending_offset: None,
            images: PageImages::default(),
            focus_handle: cx.focus_handle(),
        }
    }

    pub fn document(&self) -> &Arc<PdfDocument> {
        &self.document
    }

    pub fn zoom(&self) -> f32 {
        self.zoom
    }

    /// 現在のスクロール位置(正の値 = 内容を上/左にずらした量)。保留中の位置があればそちらを返す。
    fn scroll_position(&self) -> (f32, f32) {
        let offset = self
            .pending_offset
            .unwrap_or_else(|| self.scroll_handle.offset());
        (-f32::from(offset.x), -f32::from(offset.y))
    }

    fn viewport_size(&self, window: &Window) -> (f32, f32) {
        let bounds = self.scroll_handle.bounds().size;
        let (w, h) = (f32::from(bounds.width), f32::from(bounds.height));
        if w > 0.0 && h > 0.0 {
            (w, h)
        } else {
            // 初回描画前はまだ測れていないので、ウィンドウ全体で代用する。
            let size = window.viewport_size();
            (f32::from(size.width), f32::from(size.height))
        }
    }

    fn layout(&self, viewport_width: f32) -> Layout {
        layout::compute(&self.page_sizes, self.zoom, viewport_width)
    }

    fn current_page(&self, layout: &Layout, viewport_height: f32) -> usize {
        let (_, top) = self.scroll_position();
        layout.page_at(top + viewport_height * CURRENT_PAGE_PROBE)
    }

    fn set_scroll(&mut self, x: f32, y: f32, cx: &mut Context<Self>) {
        self.pending_offset = Some(point(px(-x), px(-y)));
        cx.notify();
    }

    /// 倍率を変える。`anchor`(ビューポート内の座標)にある内容が動かないようにスクロールも補正する。
    fn set_zoom(&mut self, new_zoom: f32, anchor: (f32, f32), cx: &mut Context<Self>) {
        let new_zoom = zoom::clamp(new_zoom);
        if new_zoom == self.zoom {
            return;
        }
        let (x, y) = self.scroll_position();
        let x = zoom::scroll_after_zoom(x, anchor.0, self.zoom, new_zoom);
        let y = zoom::scroll_after_zoom(y, anchor.1, self.zoom, new_zoom);
        self.zoom = new_zoom;
        self.set_scroll(x.max(0.0), y.max(0.0), cx);
    }

    fn viewport_center(&self, window: &Window) -> (f32, f32) {
        let (w, h) = self.viewport_size(window);
        (w / 2.0, h / 2.0)
    }

    fn zoom_in(&mut self, _: &ZoomIn, window: &mut Window, cx: &mut Context<Self>) {
        self.set_zoom(
            self.zoom * zoom::ZOOM_STEP,
            self.viewport_center(window),
            cx,
        );
    }

    fn zoom_out(&mut self, _: &ZoomOut, window: &mut Window, cx: &mut Context<Self>) {
        self.set_zoom(
            self.zoom / zoom::ZOOM_STEP,
            self.viewport_center(window),
            cx,
        );
    }

    fn reset_zoom(&mut self, _: &ResetZoom, window: &mut Window, cx: &mut Context<Self>) {
        self.set_zoom(zoom::DEFAULT_ZOOM, self.viewport_center(window), cx);
    }

    fn scroll_to_page(&mut self, index: usize, window: &Window, cx: &mut Context<Self>) {
        let (viewport_width, _) = self.viewport_size(window);
        let y = self.layout(viewport_width).scroll_top_of(index);
        let (x, _) = self.scroll_position();
        self.set_scroll(x, y, cx);
    }

    fn next_page(&mut self, _: &NextPage, window: &mut Window, cx: &mut Context<Self>) {
        let (w, h) = self.viewport_size(window);
        let current = self.current_page(&self.layout(w), h);
        self.scroll_to_page(current + 1, window, cx);
    }

    fn previous_page(&mut self, _: &PreviousPage, window: &mut Window, cx: &mut Context<Self>) {
        let (w, h) = self.viewport_size(window);
        let layout = self.layout(w);
        let current = self.current_page(&layout, h);
        // ページの途中にいるときは、まずそのページの先頭に戻る。
        let (_, top) = self.scroll_position();
        let target = if top > layout.scroll_top_of(current) + 1.0 {
            current
        } else {
            current.saturating_sub(1)
        };
        self.scroll_to_page(target, window, cx);
    }

    fn first_page(&mut self, _: &FirstPage, window: &mut Window, cx: &mut Context<Self>) {
        self.scroll_to_page(0, window, cx);
    }

    fn last_page(&mut self, _: &LastPage, window: &mut Window, cx: &mut Context<Self>) {
        self.scroll_to_page(self.page_sizes.len().saturating_sub(1), window, cx);
    }

    /// Ctrl/Cmd + ホイールでズームする。それ以外のスクロールはGPUIの標準処理に任せる。
    fn on_scroll(&mut self, event: &ScrollEvent, _window: &mut Window, cx: &mut Context<Self>) {
        if !(event.modifiers.control || event.modifiers.platform) {
            return;
        }
        let delta_y = match event.delta {
            ScrollDelta::Pixels(p) => f32::from(p.y),
            ScrollDelta::Lines(l) => l.y * WHEEL_LINE_HEIGHT,
        };
        let origin = self.scroll_handle.bounds().origin;
        let anchor = (
            f32::from(event.position.x - origin.x),
            f32::from(event.position.y - origin.y),
        );
        self.set_zoom(self.zoom * zoom::wheel_factor(delta_y), anchor, cx);
        // 標準のスクロール処理が同じイベントで動かす分を、次の描画で上書きして打ち消す。
        if self.pending_offset.is_none() {
            self.pending_offset = Some(self.scroll_handle.offset());
        }
    }
}

impl Focusable for PdfView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for PdfView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(offset) = self.pending_offset.take() {
            self.scroll_handle.set_offset(offset);
        }
        let (viewport_width, viewport_height) = self.viewport_size(window);
        let layout = self.layout(viewport_width);
        let (_, scroll_top) = self.scroll_position();
        let visible = layout.visible_range(scroll_top, viewport_height);
        self.images.retain_visible(&visible, window);

        let scale = self.zoom * window.scale_factor();
        let mut content = div().relative().w(px(layout.width)).h(px(layout.height));
        for index in visible {
            let rect = layout.pages[index];
            let page = div()
                .absolute()
                .left(px(rect.left))
                .top(px(rect.top))
                .w(px(rect.width))
                .h(px(rect.height));
            let page = match self
                .images
                .get_or_render(&self.document, index, scale, window)
            {
                Ok(image) => page.child(img(image).size_full()),
                Err(e) => page
                    .bg(rgb(0xffffff))
                    .text_color(rgb(0xcc0000))
                    .child(e.to_string()),
            };
            content = content.child(page);
        }

        let current = self.current_page(&layout, viewport_height);
        let status = format!(
            "{} / {}  ·  {:.0}%",
            current + 1,
            self.page_sizes.len(),
            self.zoom * 100.0
        );

        div()
            .key_context(KEY_CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::zoom_in))
            .on_action(cx.listener(Self::zoom_out))
            .on_action(cx.listener(Self::reset_zoom))
            .on_action(cx.listener(Self::next_page))
            .on_action(cx.listener(Self::previous_page))
            .on_action(cx.listener(Self::first_page))
            .on_action(cx.listener(Self::last_page))
            .relative()
            .size_full()
            .bg(rgb(0x3c3c3c))
            .child(
                div()
                    .id("pdf-scroll")
                    .size_full()
                    .overflow_scroll()
                    .track_scroll(&self.scroll_handle)
                    .on_scroll(cx.listener(Self::on_scroll))
                    .child(content),
            )
            .child(
                div()
                    .absolute()
                    .bottom_0()
                    .left_0()
                    .right_0()
                    .h(px(STATUS_BAR_HEIGHT))
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(rgb(0x252525))
                    .text_color(rgb(0xdddddd))
                    .text_sm()
                    .child(status),
            )
    }
}
