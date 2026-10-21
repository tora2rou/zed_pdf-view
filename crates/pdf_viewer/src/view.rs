use std::{collections::HashSet, ops::Range, sync::Arc, time::Instant};

use gpui::{
    App, AppContext as _, Context, CursorStyle, Div, FocusHandle, Focusable, MouseButton,
    MouseDownEvent, MouseMoveEvent, Pixels, Point, ScrollDelta, ScrollEvent, ScrollHandle,
    SharedString, Stateful, Task, Window, div, img, point, prelude::*, px, rgb,
};
use pdf_core::PdfDocument;

use crate::{
    actions::{
        FirstPage, KEY_CONTEXT, LastPage, NextPage, PreviousPage, ResetZoom, RotateClockwise,
        RotateCounterclockwise, ToggleDarkMode, ToggleFullscreen, ZoomIn, ZoomOut,
    },
    images::{
        ImageKey, Lookup, PageImages, RenderMode, render_page_image, settle_remaining,
        with_prefetch,
    },
    layout::{self, Layout},
    outline_rows::OutlinePath,
    page_input::PageInput,
    pan::dragged_scroll,
    select::highlight_color,
    selection::Selection,
    sidebar::SidebarTab,
    toolbar::TOOLBAR_HEIGHT,
    transform::Appearance,
    zoom::{self, ZoomMode},
};

/// ホイールが行単位(マウス)で届いたときの1行あたりのpx。
const WHEEL_LINE_HEIGHT: f32 = 40.0;
/// ページ情報の判定位置: 画面上端からこの割合だけ下にあるページを「現在のページ」とする。
const CURRENT_PAGE_PROBE: f32 = 0.3;

/// マウスのドラッグで何をするか。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Tool {
    /// ドラッグでページを動かす(スクロール)。
    #[default]
    Hand,
    /// ドラッグで文字を選択する。
    Select,
}

/// 描画結果の届け先。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RenderTarget {
    /// 本文のページ。
    Page,
    /// サイドバーのサムネイル。
    Thumbnail,
}

/// ページ1枚の描画要求。
#[derive(Debug, Clone, Copy)]
pub(crate) struct RenderRequest {
    pub index: usize,
    pub key: ImageKey,
    pub scale: f32,
    pub target: RenderTarget,
    pub mode: RenderMode,
}

/// ドラッグでスクロール中の状態。開始時のマウス位置とスクロール位置を覚えておく。
pub(crate) struct DragState {
    pub mouse: (f32, f32),
    pub scroll: (f32, f32),
}

/// PDFを表示するビュー。全ページを縦に並べてスクロール・ズームできる。
///
/// Zed本体の`image_viewer`のようにItemとして組み込めるよう、GPUIの`Render`/`Focusable`だけに
/// 依存し、ファイルの読み込みは呼び出し側が行う。
pub struct PdfView {
    pub(crate) document: Arc<PdfDocument>,
    /// 各ページのPDFポイント単位のサイズ(回転前)。
    pub(crate) base_sizes: Vec<(f32, f32)>,
    /// 各ページの表示上のサイズ(回転後)。レイアウトや倍率の計算にはこちらを使う。
    pub(crate) page_sizes: Vec<(f32, f32)>,
    /// 回転と白黒反転。
    pub(crate) appearance: Appearance,
    pub(crate) zoom: f32,
    pub(crate) zoom_mode: ZoomMode,
    pub(crate) scroll_handle: ScrollHandle,
    /// 次の描画の冒頭で適用するスクロール位置(GPUIのスクロール処理との競合を避けるため遅延させる)。
    pub(crate) pending_offset: Option<Point<Pixels>>,
    images: PageImages,
    /// サイドバーを表示しているか。
    pub(crate) sidebar_open: bool,
    pub(crate) sidebar_tab: SidebarTab,
    /// サムネイルの画像(本文の画像とは別に持つ)。
    pub(crate) thumbs: PageImages,
    pub(crate) thumb_scroll: ScrollHandle,
    pub(crate) outline_scroll: ScrollHandle,
    /// 開いている目次の項目。
    pub(crate) outline_expanded: HashSet<OutlinePath>,
    /// サムネイル一覧を最後に追従させた現在のページ。`None`なら次の描画で追従し直す。
    pub(crate) thumb_followed_page: Option<usize>,
    /// 前回の描画で使ったページ領域の大きさ。変わったら描画し直す(フィット表示などを追従させる)。
    last_viewport: (f32, f32),
    /// 最後にズーム倍率が変わった時刻。連続操作の途中で無駄な再描画をしないために使う。
    last_zoom_change: Option<Instant>,
    /// ズームが落ち着いたら再描画するための待ち時間タスク。
    settle_task: Option<Task<()>>,
    pub(crate) focus_handle: FocusHandle,
    pub(crate) drag: Option<DragState>,
    /// マウスがリンクの上にあるか(カーソルの形を変える)。
    pub(crate) hovering_link: bool,
    /// ドラッグの操作(手のひら / 文字の選択)。
    pub(crate) tool: Tool,
    /// 選択範囲(文字の選択ツールで選んだもの)。
    pub(crate) selection: Option<Selection>,
    /// 選択のドラッグ中か。
    pub(crate) selecting: bool,
    /// 最後に分かっているマウスの位置(スクロールしても選択の端を追従させるため)。
    pub(crate) last_pointer: Option<Point<Pixels>>,
    /// リンクの読み込みを、すでにバックグラウンドで始めたページ。
    pub(crate) links_requested: HashSet<usize>,
    /// 画面下部にしばらく出す短いお知らせ(リンクを開いたことなど)。
    pub(crate) notice: Option<SharedString>,
    pub(crate) notice_task: Option<Task<()>>,
    /// ページ番号入力欄の編集状態。`Some`の間は入力欄がフォーカスを持つ。
    pub(crate) page_input: Option<PageInput>,
    pub(crate) input_focus: FocusHandle,
}

/// 各ページのPDFポイント単位のサイズ。読み取れないページは0。
fn page_sizes_of(document: &PdfDocument) -> Vec<(f32, f32)> {
    (0..document.page_count())
        .map(|i| document.page_size(i).unwrap_or((0.0, 0.0)))
        .collect()
}

impl PdfView {
    pub fn new(document: Arc<PdfDocument>, cx: &mut Context<Self>) -> Self {
        let base_sizes = page_sizes_of(&document);
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
            sidebar_open: false,
            sidebar_tab: SidebarTab::default(),
            thumbs: PageImages::default(),
            thumb_scroll: ScrollHandle::new(),
            outline_scroll: ScrollHandle::new(),
            outline_expanded: HashSet::new(),
            thumb_followed_page: None,
            last_viewport: (0.0, 0.0),
            last_zoom_change: None,
            settle_task: None,
            focus_handle: cx.focus_handle(),
            drag: None,
            hovering_link: false,
            tool: Tool::default(),
            selection: None,
            selecting: false,
            last_pointer: None,
            links_requested: HashSet::new(),
            notice: None,
            notice_task: None,
            page_input: None,
            input_focus: cx.focus_handle(),
        }
    }

    /// 表示する文書を入れ替える。先頭ページに戻り、ズームや回転などの表示設定は引き継ぐ。
    pub fn set_document(
        &mut self,
        document: Arc<PdfDocument>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for old in self.images.clear().into_iter().chain(self.thumbs.clear()) {
            window.drop_image(old).ok();
        }
        self.outline_expanded.clear();
        self.links_requested.clear();
        self.selection = None;
        self.selecting = false;
        self.thumb_followed_page = None;
        self.base_sizes = page_sizes_of(&document);
        self.page_sizes = self
            .base_sizes
            .iter()
            .map(|&size| self.appearance.rotation.apply_to_size(size))
            .collect();
        self.document = document;
        self.page_input = None;
        self.drag = None;
        self.set_scroll(0.0, 0.0, cx);
    }

    pub fn document(&self) -> &Arc<PdfDocument> {
        &self.document
    }

    pub fn zoom(&self) -> f32 {
        self.zoom
    }

    /// 現在のスクロール位置(正の値 = 内容を上/左にずらした量)。保留中の位置があればそちらを返す。
    pub(crate) fn scroll_position(&self) -> (f32, f32) {
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
        self.last_zoom_change = Some(cx.background_executor().now());
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

    /// ページ領域でマウスを押したら、ビューにフォーカスを戻してドラッグスクロールを始める。
    fn start_drag(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus_handle, cx);
        // クリックかドラッグかの判定のために、どちらのツールでも押した位置を覚えておく。
        self.drag = Some(DragState {
            mouse: (f32::from(event.position.x), f32::from(event.position.y)),
            scroll: self.scroll_position(),
        });
        if self.tool == Tool::Select {
            self.begin_selection(event, window, cx);
        }
        cx.notify();
    }

    fn drag_to(&mut self, event: &MouseMoveEvent, window: &Window, cx: &mut Context<Self>) {
        let Some(drag) = self.drag.as_ref() else {
            return;
        };
        if !event.dragging() {
            // ボタンを離したのを取り逃した(ウィンドウ外で離したなど)ので終わりにする。
            self.drag = None;
            self.selecting = false;
            cx.notify();
            return;
        }
        if self.tool == Tool::Select {
            // 選択ツールでは、ページを動かさず、選択の端を動かす。
            self.extend_selection(event.position, window, cx);
            return;
        }
        let max = self.scroll_handle.max_offset();
        let (x, y) = dragged_scroll(
            drag.scroll,
            drag.mouse,
            (f32::from(event.position.x), f32::from(event.position.y)),
            (f32::from(max.x), f32::from(max.y)),
        );
        self.set_scroll(x, y, cx);
    }

    fn end_drag(&mut self, cx: &mut Context<Self>) {
        self.selecting = false;
        if self.drag.take().is_some() {
            cx.notify();
        }
    }

    /// ページ`index`の描画をバックグラウンドで始め、終わったら結果を受け取って再描画する。
    pub(crate) fn spawn_render(
        &mut self,
        request: RenderRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let RenderRequest {
            index,
            key,
            scale,
            target,
            mode,
        } = request;
        let document = self.document.clone();
        let appearance = self.appearance;
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    render_page_image(&document, index, scale, appearance, mode)
                })
                .await;
            this.update_in(cx, |this, window, cx| {
                let images = match target {
                    RenderTarget::Page => &mut this.images,
                    RenderTarget::Thumbnail => &mut this.thumbs,
                };
                if let Some(unused) = images.complete(index, key, result) {
                    // 置き換わった古い画像、または古くなって使われなかった画像。
                    window.drop_image(unused).ok();
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// ズームが落ち着く頃に再描画させる(その時点で改めて高解像度の描画を要求するため)。
    fn schedule_settle(
        &mut self,
        remaining: std::time::Duration,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.settle_task = Some(cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(remaining).await;
            this.update(cx, |_, cx| cx.notify()).ok();
        }));
    }

    /// ズーム方式を切り替える。`Manual`以外は次の描画で倍率が計算される。
    pub(crate) fn set_zoom_mode(&mut self, mode: ZoomMode, cx: &mut Context<Self>) {
        self.zoom_mode = mode;
        cx.notify();
    }

    /// フィット表示の間、ビューポートの大きさに合わせて倍率を計算し直す。
    /// 倍率が変わったら、スクロール位置も内容に合わせて補正する。
    fn apply_fit(&mut self, viewport: (f32, f32), now: Instant) {
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
        self.last_zoom_change = Some(now);
        let y = if self.zoom_mode == ZoomMode::FitPage {
            self.layout(viewport.0).scroll_top_of(current)
        } else {
            y * target / old
        };
        self.scroll_handle.set_offset(point(px(0.0), px(-y)));
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

impl PdfView {
    /// 見えているページを先に、続けて先読み分の描画を要求する。画面外のページの画像は破棄する。
    ///
    /// ズーム操作の直後は、仮表示できる古い画像があるページの再描画を少し待つ
    /// (連続操作で、すぐ古くなる描画をしないため)。
    fn request_page_images(
        &mut self,
        visible: &Range<usize>,
        key: ImageKey,
        scale: f32,
        now: Instant,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let keep = with_prefetch(visible, self.page_sizes.len());
        for old in self.images.retain(&keep) {
            window.drop_image(old).ok();
        }
        let settling = self
            .last_zoom_change
            .and_then(|changed| settle_remaining(now.saturating_duration_since(changed)));
        let prefetch = keep.clone().filter(|i| !visible.contains(i));
        for index in visible.clone().chain(prefetch) {
            let needs_render = match self.images.lookup(index, &key) {
                Lookup::Fresh(_) | Lookup::Failed(_) => false,
                Lookup::Stale(_) => settling.is_none(),
                Lookup::Missing => true,
            };
            if needs_render && self.images.request(index, key) {
                let request = RenderRequest {
                    index,
                    key,
                    scale,
                    target: RenderTarget::Page,
                    mode: RenderMode::Cached,
                };
                self.spawn_render(request, window, cx);
            }
        }
        match settling {
            Some(remaining) => self.schedule_settle(remaining, window, cx),
            None => self.settle_task = None,
        }
    }

    /// 見えているページを、レイアウトの位置に並べた要素にする。
    fn render_pages(&self, layout: &Layout, visible: Range<usize>, key: &ImageKey) -> Div {
        let placeholder = if self.appearance.invert {
            0x1a1a1a
        } else {
            0xffffff
        };
        let mut content = div().relative().w(px(layout.width)).h(px(layout.height));
        for index in visible {
            let rect = layout.pages[index];
            let page = div()
                .absolute()
                .left(px(rect.left))
                .top(px(rect.top))
                .w(px(rect.width))
                .h(px(rect.height));
            let page = match self.images.lookup(index, key) {
                // 古い画像は、GPUが新しいページの大きさに引き伸ばして仮に表示する。
                Lookup::Fresh(image) | Lookup::Stale(image) => page.child(img(image).size_full()),
                Lookup::Failed(message) => page
                    .bg(rgb(0xffffff))
                    .text_color(rgb(0xcc0000))
                    .child(message.to_string()),
                Lookup::Missing => page.bg(rgb(placeholder)),
            };
            // 選択範囲のハイライトを、ページの上に重ねる。
            let page = page.children(self.selection_highlights(index).into_iter().map(|r| {
                div()
                    .absolute()
                    .left(px(r.x * self.zoom))
                    .top(px(r.y * self.zoom))
                    .w(px(r.width * self.zoom))
                    .h(px(r.height * self.zoom))
                    .bg(highlight_color())
            }));
            content = content.child(page);
        }
        content
    }

    /// ページを並べた`content`を包む、スクロール・ズーム・ドラッグ操作を受ける領域。
    fn render_pages_area(&self, content: Div, cx: &mut Context<Self>) -> Stateful<Div> {
        div()
            .id("pdf-scroll")
            .flex_1()
            .min_w_0()
            .h_full()
            .overflow_scroll()
            .track_scroll(&self.scroll_handle)
            .on_scroll(cx.listener(Self::on_scroll))
            .cursor(if self.hovering_link {
                CursorStyle::PointingHand
            } else if self.tool == Tool::Select {
                CursorStyle::IBeam
            } else if self.drag.is_some() {
                CursorStyle::ClosedHand
            } else {
                CursorStyle::OpenHand
            })
            .on_mouse_down(MouseButton::Left, cx.listener(Self::start_drag))
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                this.drag_to(event, window, cx);
                this.update_link_hover(event.position, window, cx);
            }))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::finish_press))
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| this.end_drag(cx)),
            )
            .child(content)
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
        if viewport != self.last_viewport {
            // ページ領域の大きさが変わった(ウィンドウのリサイズ、サイドバーの開閉など)。
            // 測った大きさは次のフレームで使えるようになるので、もう一度描画して追従させる。
            self.last_viewport = viewport;
            cx.notify();
        }
        let now = cx.background_executor().now();
        self.apply_fit(viewport, now);
        let (viewport_width, viewport_height) = viewport;
        let layout = self.layout(viewport_width);
        let (_, scroll_top) = self.scroll_position();
        // 勢いよくスクロールした直後の範囲外の位置を、そのまま使わない(画像を全部捨てて点滅するため)。
        let scroll_top = layout.clamp_scroll_top(scroll_top, viewport_height);
        let visible = layout.visible_range(scroll_top, viewport_height);
        let scale = self.zoom * window.scale_factor();
        let key = self.images.key(scale, self.appearance);
        self.request_page_images(&visible, key, scale, now, window, cx);
        self.request_page_links(&visible, window, cx);
        self.autoscroll_selection(window, cx);

        let content = self.render_pages(&layout, visible, &key);

        let current = self.current_page(&layout, viewport_height);
        let toolbar = self.render_toolbar(current, cx);
        // 入力欄の編集中は、矢印キーなどがページ移動に奪われないようキーコンテキストを外す。
        let editing = self.page_input.is_some();

        let sidebar = self
            .sidebar_open
            .then(|| self.render_sidebar(current, window, cx));
        let pages_area = self.render_pages_area(content, cx);

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
            .on_action(cx.listener(Self::toggle_sidebar))
            .on_action(cx.listener(Self::copy_selection))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::clear_selection))
            .on_action(cx.listener(Self::hand_tool))
            .on_action(cx.listener(Self::select_tool))
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(0x3c3c3c))
            .child(toolbar)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .children(sidebar)
                    .child(pages_area),
            )
            .children(self.render_notice())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transform::Appearance;
    use gpui::TestAppContext;

    const FIXTURE: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../pdf_core/tests/fixtures/two_pages.pdf"
    );

    fn open_view(cx: &mut TestAppContext) -> (gpui::Entity<PdfView>, &mut gpui::VisualTestContext) {
        let document = Arc::new(PdfDocument::open(FIXTURE).unwrap());
        cx.add_window_view(|_, cx| PdfView::new(document, cx))
    }

    /// 描画はバックグラウンドで行われ、終わると画像が届く。見えているページも先読みのページも。
    #[gpui::test]
    fn pages_are_rendered_in_the_background(cx: &mut TestAppContext) {
        let (view, cx) = open_view(cx);
        cx.run_until_parked();

        let fresh = view.update_in(cx, |this, window, _| {
            let key = this
                .images
                .key(this.zoom * window.scale_factor(), Appearance::default());
            (0..2)
                .map(|i| matches!(this.images.lookup(i, &key), Lookup::Fresh(_)))
                .collect::<Vec<_>>()
        });
        assert_eq!(fresh, [true, true], "pages 0 and 1 should be rendered");
    }

    /// 文書を入れ替えると、古い文書の画像は残らず、新しい文書が描画される。
    #[gpui::test]
    fn replacing_the_document_renders_the_new_one(cx: &mut TestAppContext) {
        let (view, cx) = open_view(cx);
        cx.run_until_parked();

        let other = Arc::new(PdfDocument::open(FIXTURE).unwrap());
        view.update_in(cx, |this, window, cx| this.set_document(other, window, cx));
        cx.run_until_parked();

        let fresh = view.update_in(cx, |this, window, _| {
            let key = this
                .images
                .key(this.zoom * window.scale_factor(), Appearance::default());
            matches!(this.images.lookup(0, &key), Lookup::Fresh(_))
        });
        assert!(fresh, "the new document's page 0 should be rendered");
    }

    /// ズームすると、再描画が終わるまで古い画像を仮表示し、落ち着いたら新しい倍率で描画し直す。
    #[gpui::test]
    fn zoom_rerenders_after_settling(cx: &mut TestAppContext) {
        let (view, cx) = open_view(cx);
        cx.run_until_parked();

        view.update_in(cx, |this, window, cx| {
            this.zoom_in(&ZoomIn, window, cx);
        });
        let stale = view.update_in(cx, |this, window, _| {
            let key = this
                .images
                .key(this.zoom * window.scale_factor(), Appearance::default());
            matches!(
                this.images.lookup(0, &key),
                Lookup::Stale(_) | Lookup::Fresh(_)
            )
        });
        assert!(stale, "an image should still be shown right after zooming");

        // 待ち時間のタイマーも含めて進める
        cx.executor().advance_clock(crate::images::ZOOM_SETTLE * 2);
        cx.run_until_parked();
        let fresh = view.update_in(cx, |this, window, _| {
            let key = this
                .images
                .key(this.zoom * window.scale_factor(), Appearance::default());
            matches!(this.images.lookup(0, &key), Lookup::Fresh(_))
        });
        assert!(fresh, "page 0 should be re-rendered at the new zoom");
    }
}
