use std::sync::Arc;

use gpui::{
    App, Context, FocusHandle, Focusable, KeyDownEvent, MouseButton, Pixels, Point, ScrollDelta,
    ScrollEvent, ScrollHandle, Window, div, img, point, prelude::*, px, rgb,
};
use pdf_core::PdfDocument;

use crate::{
    actions::{
        FirstPage, KEY_CONTEXT, LastPage, NextPage, PreviousPage, ResetZoom, RotateClockwise,
        RotateCounterclockwise, ToggleDarkMode, ToggleFullscreen, ZoomIn, ZoomOut,
    },
    images::PageImages,
    layout::{self, Layout},
    page_input::PageInput,
    toolbar::TOOLBAR_HEIGHT,
    transform::Appearance,
    zoom::{self, ZoomMode},
};

/// ホイールが行単位(マウス)で届いたときの1行あたりのpx。
const WHEEL_LINE_HEIGHT: f32 = 40.0;
/// ページ情報の判定位置: 画面上端からこの割合だけ下にあるページを「現在のページ」とする。
const CURRENT_PAGE_PROBE: f32 = 0.3;

/// PDFを表示するビュー。全ページを縦に並べてスクロール・ズームできる。
///
/// Zed本体の`image_viewer`のようにItemとして組み込めるよう、GPUIの`Render`/`Focusable`だけに
/// 依存し、ファイルの読み込みは呼び出し側が行う。
pub struct PdfView {
    pub(crate) document: Arc<PdfDocument>,
    /// 各ページのPDFポイント単位のサイズ(回転前)。
    base_sizes: Vec<(f32, f32)>,
    /// 各ページの表示上のサイズ(回転後)。レイアウトや倍率の計算にはこちらを使う。
    pub(crate) page_sizes: Vec<(f32, f32)>,
    /// 回転と白黒反転。
    pub(crate) appearance: Appearance,
    pub(crate) zoom: f32,
    pub(crate) zoom_mode: ZoomMode,
    scroll_handle: ScrollHandle,
    /// 次の描画の冒頭で適用するスクロール位置(GPUIのスクロール処理との競合を避けるため遅延させる)。
    pending_offset: Option<Point<Pixels>>,
    images: PageImages,
    focus_handle: FocusHandle,
    /// ページ番号入力欄の編集状態。`Some`の間は入力欄がフォーカスを持つ。
    pub(crate) page_input: Option<PageInput>,
    pub(crate) input_focus: FocusHandle,
}

impl PdfView {
    pub fn new(document: Arc<PdfDocument>, cx: &mut Context<Self>) -> Self {
        let base_sizes: Vec<_> = (0..document.page_count())
            .map(|i| document.page_size(i).unwrap_or((0.0, 0.0)))
            .collect();
        Self {
            document,
            page_sizes: base_sizes.clone(),
            base_sizes,
            appearance: Appearance::default(),
            zoom: zoom::DEFAULT_ZOOM,
            zoom_mode: ZoomMode::Manual,
            scroll_handle: ScrollHandle::new(),
            pending_offset: None,
            images: PageImages::default(),
            focus_handle: cx.focus_handle(),
            page_input: None,
            input_focus: cx.focus_handle(),
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

    pub(crate) fn viewport_size(&self, window: &Window) -> (f32, f32) {
        let bounds = self.scroll_handle.bounds().size;
        let (w, h) = (f32::from(bounds.width), f32::from(bounds.height));
        if w > 0.0 && h > 0.0 {
            (w, h)
        } else {
            // 初回描画前はまだ測れていないので、ウィンドウ全体で代用する。
            let size = window.viewport_size();
            (
                f32::from(size.width),
                f32::from(size.height) - TOOLBAR_HEIGHT,
            )
        }
    }

    pub(crate) fn layout(&self, viewport_width: f32) -> Layout {
        layout::compute(&self.page_sizes, self.zoom, viewport_width)
    }

    pub(crate) fn current_page(&self, layout: &Layout, viewport_height: f32) -> usize {
        let (_, top) = self.scroll_position();
        layout.page_at(top + viewport_height * CURRENT_PAGE_PROBE)
    }

    pub(crate) fn set_scroll(&mut self, x: f32, y: f32, cx: &mut Context<Self>) {
        self.pending_offset = Some(point(px(-x), px(-y)));
        cx.notify();
    }

    /// 倍率を変える。`anchor`(ビューポート内の座標)にある内容が動かないようにスクロールも補正する。
    pub(crate) fn set_zoom(&mut self, new_zoom: f32, anchor: (f32, f32), cx: &mut Context<Self>) {
        self.zoom_mode = ZoomMode::Manual;
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

    pub(crate) fn viewport_center(&self, window: &Window) -> (f32, f32) {
        let (w, h) = self.viewport_size(window);
        (w / 2.0, h / 2.0)
    }

    pub(crate) fn zoom_in(&mut self, _: &ZoomIn, window: &mut Window, cx: &mut Context<Self>) {
        self.set_zoom(
            self.zoom * zoom::ZOOM_STEP,
            self.viewport_center(window),
            cx,
        );
    }

    pub(crate) fn zoom_out(&mut self, _: &ZoomOut, window: &mut Window, cx: &mut Context<Self>) {
        self.set_zoom(
            self.zoom / zoom::ZOOM_STEP,
            self.viewport_center(window),
            cx,
        );
    }

    pub(crate) fn reset_zoom(
        &mut self,
        _: &ResetZoom,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_zoom(zoom::DEFAULT_ZOOM, self.viewport_center(window), cx);
    }

    pub(crate) fn scroll_to_page(&mut self, index: usize, window: &Window, cx: &mut Context<Self>) {
        let (viewport_width, _) = self.viewport_size(window);
        let y = self.layout(viewport_width).scroll_top_of(index);
        let (x, _) = self.scroll_position();
        self.set_scroll(x, y, cx);
    }

    pub(crate) fn next_page(&mut self, _: &NextPage, window: &mut Window, cx: &mut Context<Self>) {
        let (w, h) = self.viewport_size(window);
        let current = self.current_page(&self.layout(w), h);
        self.scroll_to_page(current + 1, window, cx);
    }

    pub(crate) fn previous_page(
        &mut self,
        _: &PreviousPage,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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

    /// 回転を変える。表示サイズが変わるので、いま見ているページの先頭に合わせ直す。
    fn set_rotation(
        &mut self,
        rotation: crate::transform::Rotation,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let (w, h) = self.viewport_size(window);
        let current = self.current_page(&self.layout(w), h);
        self.appearance.rotation = rotation;
        self.page_sizes = self
            .base_sizes
            .iter()
            .map(|&size| rotation.apply_to_size(size))
            .collect();
        self.scroll_to_page(current, window, cx);
    }

    pub(crate) fn rotate_clockwise(
        &mut self,
        _: &RotateClockwise,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_rotation(self.appearance.rotation.clockwise(), window, cx);
    }

    pub(crate) fn rotate_counterclockwise(
        &mut self,
        _: &RotateCounterclockwise,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_rotation(self.appearance.rotation.counterclockwise(), window, cx);
    }

    pub(crate) fn toggle_dark_mode(
        &mut self,
        _: &ToggleDarkMode,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.appearance.invert = !self.appearance.invert;
        cx.notify();
    }

    pub(crate) fn toggle_fullscreen(
        &mut self,
        _: &ToggleFullscreen,
        window: &mut Window,
        _: &mut Context<Self>,
    ) {
        window.toggle_fullscreen();
    }

    /// ズーム方式を切り替える。`Manual`以外は次の描画で倍率が計算される。
    pub(crate) fn set_zoom_mode(&mut self, mode: ZoomMode, cx: &mut Context<Self>) {
        self.zoom_mode = mode;
        cx.notify();
    }

    /// フィット表示の間、ビューポートの大きさに合わせて倍率を計算し直す。
    /// 倍率が変わったら、スクロール位置も内容に合わせて補正する。
    fn apply_fit(&mut self, viewport: (f32, f32)) {
        let target = match self.zoom_mode {
            ZoomMode::Manual => return,
            ZoomMode::FitWidth => {
                let widest = self.page_sizes.iter().map(|s| s.0).fold(0.0, f32::max);
                zoom::fit_width(viewport.0, widest)
            }
            ZoomMode::FitPage => {
                let current = self.current_page(&self.layout(viewport.0), viewport.1);
                let size = self.page_sizes.get(current).copied().unwrap_or_default();
                zoom::fit_page(viewport, size)
            }
        };
        if (target - self.zoom).abs() < 1e-4 {
            return;
        }
        let current = self.current_page(&self.layout(viewport.0), viewport.1);
        let (_, y) = self.scroll_position();
        let old = self.zoom;
        self.zoom = target;
        let y = if self.zoom_mode == ZoomMode::FitPage {
            self.layout(viewport.0).scroll_top_of(current)
        } else {
            y * target / old
        };
        self.scroll_handle.set_offset(point(px(0.0), px(-y)));
    }

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
        if self.page_input.is_some() && !self.input_focus.is_focused(window) {
            // 入力欄の外をクリックするなどしてフォーカスが外れたら、編集をやめる。
            self.page_input = None;
        }
        if let Some(offset) = self.pending_offset.take() {
            self.scroll_handle.set_offset(offset);
        }
        let viewport = self.viewport_size(window);
        self.apply_fit(viewport);
        let (viewport_width, viewport_height) = viewport;
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
            let page = match self.images.get_or_render(
                &self.document,
                index,
                scale,
                self.appearance,
                window,
            ) {
                Ok(image) => page.child(img(image).size_full()),
                Err(e) => page
                    .bg(rgb(0xffffff))
                    .text_color(rgb(0xcc0000))
                    .child(e.to_string()),
            };
            content = content.child(page);
        }

        let current = self.current_page(&layout, viewport_height);
        let toolbar = self.render_toolbar(current, cx);
        // 入力欄の編集中は、矢印キーなどがページ移動に奪われないようキーコンテキストを外す。
        let editing = self.page_input.is_some();

        div()
            .when(!editing, |d| d.key_context(KEY_CONTEXT))
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::zoom_in))
            .on_action(cx.listener(Self::zoom_out))
            .on_action(cx.listener(Self::reset_zoom))
            .on_action(cx.listener(Self::next_page))
            .on_action(cx.listener(Self::previous_page))
            .on_action(cx.listener(Self::first_page))
            .on_action(cx.listener(Self::last_page))
            .on_action(cx.listener(Self::rotate_clockwise))
            .on_action(cx.listener(Self::rotate_counterclockwise))
            .on_action(cx.listener(Self::toggle_dark_mode))
            .on_action(cx.listener(Self::toggle_fullscreen))
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(0x3c3c3c))
            .child(toolbar)
            .child(
                div()
                    .id("pdf-scroll")
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .overflow_scroll()
                    .track_scroll(&self.scroll_handle)
                    .on_scroll(cx.listener(Self::on_scroll))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, window, cx| {
                            window.focus(&this.focus_handle, cx);
                        }),
                    )
                    .child(content),
            )
    }
}
