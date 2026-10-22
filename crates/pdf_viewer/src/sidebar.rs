//! 左側のサイドバー(ページのサムネイル一覧と目次)。

use gpui::{
    ClickEvent, Context, IntoElement, Stateful, Window, div, img, point, prelude::*, px, rgb,
};

use crate::{
    actions::ToggleSidebar,
    images::{Lookup, with_margin},
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
/// 目次の1行の高さ。開閉ボタンを押しやすいよう、少し高めにしてある。
const OUTLINE_ROW_HEIGHT: f32 = 30.0;
/// 開閉ボタン(`+` / `−`)の当たり判定の幅。行の高さいっぱいに広げた領域を押せる。
const OUTLINE_TOGGLE_WIDTH: f32 = 34.0;
/// 行の左端の余白。
const OUTLINE_ROW_PADDING: f32 = 4.0;
const OUTLINE_INDENT: f32 = 14.0;
/// 見えているサムネイルの前後に、描画しておく(捨てずに持っておく)ページ数。
/// サムネイルは小さくて軽いので、本文より多めに持ち、速くスクロールして戻っても空白を見せない。
const THUMB_MARGIN_PAGES: usize = 6;

const BACKGROUND: u32 = 0x2b2b2b;
const TAB_BACKGROUND: u32 = 0x252525;
const TEXT: u32 = 0xdddddd;
const MUTED_TEXT: u32 = 0x999999;
const HOVER: u32 = 0x3a3a3a;
const ACTIVE: u32 = 0x4a4a4a;
const ACCENT: u32 = 0x4d9bff;
/// サムネイルの画像が届くまでの枠の色。
const THUMB_PLACEHOLDER: u32 = 0x3a3a3a;

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
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (viewport_width, _) = self.viewport_size(window);
        self.sidebar_open = !self.sidebar_open;
        if !self.sidebar_open {
            // 閉じるとページ領域が広がる。拡大して横にはみ出していたら、中央に戻す。
            let widened = viewport_width + SIDEBAR_WIDTH;
            let content_width = self.layout(widened).width;
            let (_, y) = self.scroll_position();
            self.set_scroll(layout::centered_scroll_x(content_width, widened), y, cx);
        }
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

        // 勢いよくスクロールした直後の範囲外の位置を、そのまま使わない(画像を全部捨てて点滅するため)。
        let scroll_top =
            layout.clamp_scroll_top(-f32::from(self.thumb_scroll.offset().y), viewport_height);
        let visible = layout.visible_range(scroll_top, viewport_height);
        let keep = with_margin(&visible, THUMB_MARGIN_PAGES, self.page_sizes.len());
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
                .border_color(rgb(if is_current { ACCENT } else { BACKGROUND }));
            // 画像が届くまでは、白でなく暗い灰色の枠を出す(白だと、画像が現れるときに点滅して見える)。
            let picture = match self.thumbs.lookup(index, &key) {
                Lookup::Fresh(image) | Lookup::Stale(image) => {
                    picture.bg(rgb(0xffffff)).child(img(image).size_full())
                }
                Lookup::Failed(_) | Lookup::Missing => picture.bg(rgb(THUMB_PLACEHOLDER)),
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
            .w(px(OUTLINE_TOGGLE_WIDTH))
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
                    .text_base()
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
            .pl(px(OUTLINE_ROW_PADDING + row.depth as f32 * OUTLINE_INDENT))
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
        cx.simulate_click(
            point(px(OUTLINE_ROW_PADDING + OUTLINE_TOGGLE_WIDTH / 2.0), px(y)),
            Modifiers::none(),
        );
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

    /// 拡大して横にはみ出しているとき、サイドバーを閉じると本文が水平方向の中央に戻る。
    #[gpui::test]
    fn closing_the_sidebar_recenters_a_wide_page(cx: &mut TestAppContext) {
        let (view, cx) = open_view(TWO_PAGES, cx);
        view.update_in(cx, |this, _, cx| this.set_zoom(5.0, (0.0, 0.0), cx));
        cx.run_until_parked();
        open_sidebar(&view, SidebarTab::Pages, cx);

        // 左端までスクロールした状態で閉じる
        view.update_in(cx, |this, _, cx| this.set_scroll(0.0, 0.0, cx));
        cx.run_until_parked();
        assert_eq!(
            view.read_with(cx, |this, _| f32::from(this.scroll_handle.offset().x)),
            0.0
        );
        view.update_in(cx, |this, window, cx| {
            this.toggle_sidebar(&ToggleSidebar, window, cx)
        });
        cx.run_until_parked();

        let (offset, expected) = view.update_in(cx, |this, window, _| {
            let (w, _) = this.viewport_size(window);
            let content = this.layout(w).width;
            (
                f32::from(this.scroll_handle.offset().x),
                -layout::centered_scroll_x(content, w),
            )
        });
        assert!(
            expected < -100.0,
            "the page should overflow (expected {expected})"
        );
        assert!(
            (offset - expected).abs() < 1.0,
            "offset {offset}, expected {expected}"
        );
    }

    /// 開閉ボタンは、見た目の記号の周りの広い範囲(隅の近くまで)で反応する。
    #[gpui::test]
    fn toggle_responds_near_the_corners_of_its_area(cx: &mut TestAppContext) {
        let (view, cx) = open_view(WITH_OUTLINE, cx);
        open_sidebar(&view, SidebarTab::Outline, cx);
        let top = 36.0 + TAB_HEIGHT + 4.0;
        let expanded = |view: &gpui::Entity<PdfView>, cx: &mut VisualTestContext| {
            view.read_with(cx, |this, _| this.outline_expanded.contains(&vec![0]))
        };

        // ボタン領域の左上の隅と右下の隅の、それぞれ2px内側
        for (dx, dy) in [
            (2.0, 2.0),
            (OUTLINE_TOGGLE_WIDTH - 2.0, OUTLINE_ROW_HEIGHT - 2.0),
        ] {
            let before = expanded(&view, cx);
            cx.simulate_click(
                point(px(OUTLINE_ROW_PADDING + dx), px(top + dy)),
                Modifiers::none(),
            );
            cx.run_until_parked();
            assert_ne!(
                before,
                expanded(&view, cx),
                "no toggle at offset ({dx}, {dy})"
            );
        }
    }

    /// `count`ページの小さなPDFを一時ファイルに作る。
    fn make_pdf(count: usize, name: &str) -> std::path::PathBuf {
        let kids: String = (0..count).map(|i| format!("{} 0 R ", 3 + i)).collect();
        let mut objects = vec![
            "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
            format!("<< /Type /Pages /Kids [{kids}] /Count {count} >>"),
        ];
        objects.extend(
            (0..count)
                .map(|_| "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 300] >>".to_string()),
        );
        let mut pdf = String::from("%PDF-1.4\n");
        let mut offsets = Vec::new();
        for (i, object) in objects.iter().enumerate() {
            offsets.push(pdf.len());
            pdf.push_str(&format!("{} 0 obj\n{object}\nendobj\n", i + 1));
        }
        let xref = pdf.len();
        pdf.push_str(&format!(
            "xref\n0 {}\n0000000000 65535 f \n",
            objects.len() + 1
        ));
        for offset in offsets {
            pdf.push_str(&format!("{offset:010} 00000 n \n"));
        }
        pdf.push_str(&format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        ));
        let path =
            std::env::temp_dir().join(format!("pdf_viewer_{name}_{}.pdf", std::process::id()));
        std::fs::write(&path, pdf).unwrap();
        path
    }

    /// 末尾を超えた(範囲外の)スクロール位置で描画されても、サムネイルの画像を捨てない。
    /// (捨ててしまうと、位置が戻った次のフレームで全部描き直しになり、画面が点滅する。)
    #[gpui::test]
    fn overscrolling_the_thumbnail_list_keeps_its_images(cx: &mut TestAppContext) {
        let path = make_pdf(40, "overscroll");
        let (view, cx) = open_view(path.to_str().unwrap(), cx);
        open_sidebar(&view, SidebarTab::Pages, cx);

        // 末尾までスクロールして、末尾のサムネイルが描画されるのを待つ
        view.update_in(cx, |this, _, cx| {
            this.thumb_scroll.set_offset(point(px(0.0), px(-1.0e6)));
            cx.notify();
        });
        cx.run_until_parked();
        let last = 39;
        let is_fresh = |view: &gpui::Entity<PdfView>, cx: &mut VisualTestContext| {
            view.update_in(cx, |this, window, _| {
                let scale = thumbnail_scale(this.page_sizes[last].0, window.scale_factor());
                let key = this.thumbs.key(scale, Appearance::default());
                matches!(this.thumbs.lookup(last, &key), Lookup::Fresh(_))
            })
        };
        assert!(is_fresh(&view, cx), "the last thumbnail should be rendered");

        // 範囲外の位置にして、1フレームだけ描画する(その間は描画タスクを進めない)
        view.update_in(cx, |this, window, cx| {
            this.thumb_scroll.set_offset(point(px(0.0), px(-1.0e6)));
            cx.notify();
            let _ = window;
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert!(
            is_fresh(&view, cx),
            "the last thumbnail was dropped while overscrolled"
        );
        std::fs::remove_file(path).ok();
    }
}
