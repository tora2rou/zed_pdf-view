//! 左側のサイドバー(ページのサムネイル一覧と目次)。

use gpui::{
    ClickEvent, Context, IntoElement, Stateful, Window, div, img, point, prelude::*, px, rgb,
};

use crate::{
    actions::ToggleSidebar,
    images::{Lookup, RenderMode, with_prefetch},
    layout::{self, MARGIN},
    outline_rows::{self, OutlineRow},
    view::{PdfView, RenderRequest, RenderTarget},
};

pub const SIDEBAR_WIDTH: f32 = 220.0;
/// サムネイル画像の幅(論理px)。
const THUMB_WIDTH: f32 = 150.0;
/// サムネイルの下に出すページ番号の行の高さ。
const LABEL_HEIGHT: f32 = 20.0;
const TAB_HEIGHT: f32 = 30.0;
const OUTLINE_ROW_HEIGHT: f32 = 24.0;
const OUTLINE_INDENT: f32 = 14.0;

const BACKGROUND: u32 = 0x2b2b2b;
const TAB_BACKGROUND: u32 = 0x252525;
const TEXT: u32 = 0xdddddd;
const MUTED_TEXT: u32 = 0x999999;
const HOVER: u32 = 0x3a3a3a;
const ACTIVE: u32 = 0x4a4a4a;
const ACCENT: u32 = 0x4d9bff;

/// サイドバーに出す内容。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SidebarTab {
    #[default]
    Pages,
    Outline,
}

/// サムネイル1枚分のセル(画像 + ページ番号の行)の大きさ。幅は固定で、高さはページの縦横比による。
pub fn thumbnail_cell_sizes(page_sizes: &[(f32, f32)]) -> Vec<(f32, f32)> {
    page_sizes
        .iter()
        .map(|&(w, h)| {
            let image_height = if w > 0.0 && h > 0.0 {
                h * THUMB_WIDTH / w
            } else {
                THUMB_WIDTH * 1.4
            };
            (THUMB_WIDTH, image_height + LABEL_HEIGHT)
        })
        .collect()
}

/// サムネイルを描画する倍率(画面のDPI込み)。幅が不明なページは等倍。
pub fn thumbnail_scale(page_width: f32, dpi_scale: f32) -> f32 {
    if page_width > 0.0 {
        THUMB_WIDTH / page_width * dpi_scale
    } else {
        dpi_scale
    }
}

fn tab_button(id: &'static str, label: &'static str, active: bool) -> Stateful<gpui::Div> {
    div()
        .id(id)
        .flex_1()
        .h_full()
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .when(active, |d| d.bg(rgb(ACTIVE)))
        .hover(|d| d.bg(rgb(HOVER)))
        .child(label)
}

impl PdfView {
    pub(crate) fn toggle_sidebar(
        &mut self,
        _: &ToggleSidebar,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.sidebar_open = !self.sidebar_open;
        // 開き直したときは、現在のページのサムネイルまでスクロールし直す。
        self.thumb_followed_page = None;
        cx.notify();
    }

    fn set_sidebar_tab(&mut self, tab: SidebarTab, cx: &mut Context<Self>) {
        self.sidebar_tab = tab;
        self.thumb_followed_page = None;
        cx.notify();
    }

    /// サイドバー全体(タブ + 内容)。`current`は現在のページ(0始まり)。
    pub(crate) fn render_sidebar(
        &mut self,
        current: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let tab = self.sidebar_tab;
        let body = match tab {
            SidebarTab::Pages => self
                .render_thumbnails(current, window, cx)
                .into_any_element(),
            SidebarTab::Outline => self.render_outline(cx).into_any_element(),
        };
        let tabs = div()
            .flex()
            .flex_none()
            .w_full()
            .h(px(TAB_HEIGHT))
            .bg(rgb(TAB_BACKGROUND))
            .text_sm()
            .child(
                tab_button("sidebar-tab-pages", "Pages", tab == SidebarTab::Pages).on_click(
                    cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.set_sidebar_tab(SidebarTab::Pages, cx)
                    }),
                ),
            )
            .child(
                tab_button("sidebar-tab-outline", "Outline", tab == SidebarTab::Outline).on_click(
                    cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.set_sidebar_tab(SidebarTab::Outline, cx)
                    }),
                ),
            );
        div()
            .flex()
            .flex_col()
            .flex_none()
            .w(px(SIDEBAR_WIDTH))
            .h_full()
            .bg(rgb(BACKGROUND))
            .text_color(rgb(TEXT))
            .child(tabs)
            .child(div().flex_1().min_h_0().w_full().child(body))
    }

    /// サムネイル一覧。見えている範囲(と前後1枚)だけを、バックグラウンドで描画する。
    fn render_thumbnails(
        &mut self,
        current: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let layout = layout::compute(&thumbnail_cell_sizes(&self.page_sizes), 1.0, SIDEBAR_WIDTH);
        let bounds = self.thumb_scroll.bounds().size;
        let viewport_height = if f32::from(bounds.height) > 0.0 {
            f32::from(bounds.height)
        } else {
            f32::from(window.viewport_size().height)
        };

        // 現在のページが変わったら、そのサムネイルが見える位置までスクロールする。
        if self.thumb_followed_page != Some(current) {
            self.thumb_followed_page = Some(current);
            if let Some(rect) = layout.pages.get(current) {
                let top = -f32::from(self.thumb_scroll.offset().y);
                if rect.top < top || rect.bottom() > top + viewport_height {
                    let y = (rect.top - MARGIN).max(0.0);
                    self.thumb_scroll.set_offset(point(px(0.0), px(-y)));
                }
            }
        }

        let scroll_top = -f32::from(self.thumb_scroll.offset().y);
        let visible = layout.visible_range(scroll_top, viewport_height);
        let keep = with_prefetch(&visible, self.page_sizes.len());
        for old in self.thumbs.retain(&keep) {
            window.drop_image(old).ok();
        }

        let dpi = window.scale_factor();
        let appearance = self.appearance;
        let prefetch = keep.clone().filter(|i| !visible.contains(i));
        for index in visible.clone().chain(prefetch) {
            let scale = thumbnail_scale(self.page_sizes[index].0, dpi);
            let key = self.thumbs.key(scale, appearance);
            let needs_render = matches!(
                self.thumbs.lookup(index, &key),
                Lookup::Missing | Lookup::Stale(_)
            );
            if needs_render && self.thumbs.request(index, key) {
                let request = RenderRequest {
                    index,
                    key,
                    scale,
                    target: RenderTarget::Thumbnail,
                    mode: RenderMode::Uncached,
                };
                self.spawn_render(request, window, cx);
            }
        }

        let mut content = div().relative().w_full().h(px(layout.height));
        for index in visible {
            let rect = layout.pages[index];
            let scale = thumbnail_scale(self.page_sizes[index].0, dpi);
            let key = self.thumbs.key(scale, appearance);
            let is_current = index == current;
            let image_height = rect.height - LABEL_HEIGHT;
            let picture = div()
                .w(px(rect.width))
                .h(px(image_height))
                .border_2()
                .border_color(rgb(if is_current { ACCENT } else { BACKGROUND }))
                .bg(rgb(0xffffff));
            let picture = match self.thumbs.lookup(index, &key) {
                Lookup::Fresh(image) | Lookup::Stale(image) => {
                    picture.child(img(image).size_full())
                }
                Lookup::Failed(_) | Lookup::Missing => picture,
            };
            content = content.child(
                div()
                    .id(("thumbnail", index))
                    .absolute()
                    .left(px(rect.left))
                    .top(px(rect.top))
                    .w(px(rect.width))
                    .h(px(rect.height))
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        this.scroll_to_page(index, window, cx)
                    }))
                    .child(picture)
                    .child(
                        div()
                            .h(px(LABEL_HEIGHT))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_xs()
                            .text_color(rgb(if is_current { ACCENT } else { MUTED_TEXT }))
                            .child((index + 1).to_string()),
                    ),
            );
        }

        div()
            .id("thumbnail-list")
            .size_full()
            .overflow_y_scroll()
            .track_scroll(&self.thumb_scroll)
            .child(content)
    }

    /// 目次。項目をクリックするとそのページへ移動し、左の`+`/`−`で子の開閉をする。
    fn render_outline(&mut self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let rows = outline_rows::flatten(self.document.outline(), &self.outline_expanded);
        let list = div()
            .id("outline-list")
            .size_full()
            .overflow_y_scroll()
            .track_scroll(&self.outline_scroll);
        if rows.is_empty() {
            return list
                .p(px(12.0))
                .text_sm()
                .text_color(rgb(MUTED_TEXT))
                .child("No outline in this document");
        }
        list.py(px(4.0)).text_sm().children(
            rows.into_iter()
                .enumerate()
                .map(|(index, row)| self.render_outline_row(index, row, cx)),
        )
    }

    fn render_outline_row(
        &self,
        index: usize,
        row: OutlineRow,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let path = row.path.clone();
        let toggle = div()
            .id(("outline-toggle", index))
            .flex_none()
            .w(px(18.0))
            .h_full()
            .flex()
            .items_center()
            .justify_center()
            .text_color(rgb(MUTED_TEXT))
            .when(row.has_children, |d| {
                d.cursor_pointer()
                    .hover(|d| d.text_color(rgb(TEXT)))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        outline_rows::toggle(&mut this.outline_expanded, &path);
                        cx.notify();
                    }))
                    .child(if row.expanded { "−" } else { "+" })
            });
        let page = row.page;
        let title = div()
            .id(("outline-title", index))
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .items_center()
            .overflow_hidden()
            .when_some(page, |d, page| {
                d.cursor_pointer()
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        this.scroll_to_page(page, window, cx)
                    }))
            })
            .when(page.is_none(), |d| d.text_color(rgb(MUTED_TEXT)))
            .child(row.title);
        div()
            .flex()
            .items_center()
            .w_full()
            .h(px(OUTLINE_ROW_HEIGHT))
            .pl(px(6.0 + row.depth as f32 * OUTLINE_INDENT))
            .pr(px(8.0))
            .hover(|d| d.bg(rgb(HOVER)))
            .child(toggle)
            .child(title)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cells_keep_a_fixed_width_and_follow_the_aspect_ratio() {
        let cells = thumbnail_cell_sizes(&[(100.0, 200.0), (200.0, 100.0)]);
        assert_eq!(cells[0], (THUMB_WIDTH, THUMB_WIDTH * 2.0 + LABEL_HEIGHT));
        assert_eq!(cells[1], (THUMB_WIDTH, THUMB_WIDTH * 0.5 + LABEL_HEIGHT));
    }

    #[test]
    fn unreadable_page_sizes_get_a_default_shape() {
        let cells = thumbnail_cell_sizes(&[(0.0, 0.0)]);
        assert_eq!(cells[0], (THUMB_WIDTH, THUMB_WIDTH * 1.4 + LABEL_HEIGHT));
    }

    #[test]
    fn thumbnail_scale_fits_the_width_and_includes_dpi() {
        assert_eq!(thumbnail_scale(300.0, 1.0), THUMB_WIDTH / 300.0);
        assert_eq!(thumbnail_scale(300.0, 2.0), THUMB_WIDTH / 300.0 * 2.0);
        assert_eq!(thumbnail_scale(0.0, 2.0), 2.0);
    }

    use crate::transform::Appearance;
    use gpui::{Modifiers, TestAppContext, VisualTestContext};
    use pdf_core::PdfDocument;
    use std::sync::Arc;

    const TWO_PAGES: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../pdf_core/tests/fixtures/two_pages.pdf"
    );
    const WITH_OUTLINE: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../pdf_core/tests/fixtures/with_outline.pdf"
    );

    fn open_view<'a>(
        path: &str,
        cx: &'a mut TestAppContext,
    ) -> (gpui::Entity<PdfView>, &'a mut VisualTestContext) {
        let document = Arc::new(PdfDocument::open(path).unwrap());
        cx.add_window_view(|_, cx| PdfView::new(document, cx))
    }

    fn open_sidebar(view: &gpui::Entity<PdfView>, tab: SidebarTab, cx: &mut VisualTestContext) {
        view.update_in(cx, |this, window, cx| {
            this.toggle_sidebar(&ToggleSidebar, window, cx);
            this.set_sidebar_tab(tab, cx);
        });
        cx.run_until_parked();
    }

    /// サイドバーを開くと、見えているサムネイルがバックグラウンドで描画される。
    #[gpui::test]
    fn opening_the_sidebar_renders_thumbnails(cx: &mut TestAppContext) {
        let (view, cx) = open_view(TWO_PAGES, cx);
        open_sidebar(&view, SidebarTab::Pages, cx);

        let fresh = view.update_in(cx, |this, window, _| {
            (0..2)
                .map(|i| {
                    let scale = thumbnail_scale(this.page_sizes[i].0, window.scale_factor());
                    let key = this.thumbs.key(scale, Appearance::default());
                    matches!(this.thumbs.lookup(i, &key), Lookup::Fresh(_))
                })
                .collect::<Vec<_>>()
        });
        assert_eq!(fresh, [true, true]);
    }

    /// 閉じている間はサムネイルを描画しない(無駄な負荷をかけない)。
    #[gpui::test]
    fn closed_sidebar_renders_no_thumbnails(cx: &mut TestAppContext) {
        let (view, cx) = open_view(TWO_PAGES, cx);
        cx.run_until_parked();
        let any = view.update_in(cx, |this, window, _| {
            let scale = thumbnail_scale(this.page_sizes[0].0, window.scale_factor());
            let key = this.thumbs.key(scale, Appearance::default());
            !matches!(this.thumbs.lookup(0, &key), Lookup::Missing)
        });
        assert!(!any);
    }

    /// 目次の項目をクリックすると、そのページへスクロールする。
    #[gpui::test]
    fn clicking_an_outline_entry_scrolls_to_its_page(cx: &mut TestAppContext) {
        let (view, cx) = open_view(WITH_OUTLINE, cx);
        // ページが小さいままでは全体が画面に収まってスクロールできないので、拡大しておく。
        view.update_in(cx, |this, _, cx| this.set_zoom(5.0, (0.0, 0.0), cx));
        open_sidebar(&view, SidebarTab::Outline, cx);

        // 2番目の項目 "Chapter 2"(ページ3)の題名をクリックする。
        // ツールバー(36) + タブ(30) + 余白(4) の下に、行(24)が並ぶ。
        let y = 36.0 + TAB_HEIGHT + 4.0 + OUTLINE_ROW_HEIGHT * 1.5;
        cx.simulate_click(point(px(90.0), px(y)), Modifiers::none());
        cx.run_until_parked();

        let offset = view.read_with(cx, |this, _| f32::from(this.scroll_handle.offset().y));
        // ページ3の先頭までスクロールした = オフセットは負になる
        assert!(offset < 0.0, "did not scroll (offset y = {offset})");
    }

    /// 子のある項目は、`+`をクリックすると開いて子が見える。
    #[gpui::test]
    fn toggle_expands_children(cx: &mut TestAppContext) {
        let (view, cx) = open_view(WITH_OUTLINE, cx);
        open_sidebar(&view, SidebarTab::Outline, cx);
        let rows = |view: &gpui::Entity<PdfView>, cx: &mut VisualTestContext| {
            view.read_with(cx, |this, _| {
                outline_rows::flatten(this.document.outline(), &this.outline_expanded).len()
            })
        };
        assert_eq!(rows(&view, cx), 4);

        // 先頭の行("Chapter 1")の開閉ボタン
        let y = 36.0 + TAB_HEIGHT + 4.0 + OUTLINE_ROW_HEIGHT * 0.5;
        cx.simulate_click(point(px(6.0 + 9.0), px(y)), Modifiers::none());
        cx.run_until_parked();
        assert_eq!(rows(&view, cx), 5);
    }

    /// サムネイルをクリックすると、そのページへスクロールする。
    #[gpui::test]
    fn clicking_a_thumbnail_scrolls_to_its_page(cx: &mut TestAppContext) {
        let (view, cx) = open_view(WITH_OUTLINE, cx);
        view.update_in(cx, |this, _, cx| this.set_zoom(5.0, (0.0, 0.0), cx));
        open_sidebar(&view, SidebarTab::Pages, cx);

        // 200x100のページは、サムネイル画像が150x75、セルが150x95。
        // ツールバー(36) + タブ(30) の下に、余白(12)をあけて1枚目、隙間(12)をあけて2枚目が並ぶ。
        let second_thumbnail_y = 36.0 + TAB_HEIGHT + MARGIN + 95.0 + 12.0 + 40.0;
        cx.simulate_click(point(px(110.0), px(second_thumbnail_y)), Modifiers::none());
        cx.run_until_parked();

        let offset = view.read_with(cx, |this, _| f32::from(this.scroll_handle.offset().y));
        assert!(offset < 0.0, "did not scroll (offset y = {offset})");
    }
}
