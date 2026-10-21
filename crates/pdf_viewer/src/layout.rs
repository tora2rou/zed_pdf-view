//! 全ページを縦に並べたときの座標計算。GPUIに依存しない純粋関数だけを置く。

use std::ops::Range;

/// ページ間の隙間(論理px)。
pub const PAGE_GAP: f32 = 12.0;
/// 全体の外側の余白(論理px)。
pub const MARGIN: f32 = 12.0;

/// 1ページ分の配置(論理px、コンテンツ左上が原点)。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageRect {
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub height: f32,
}

impl PageRect {
    pub fn bottom(&self) -> f32 {
        self.top + self.height
    }
}

/// 全ページの配置。コンテンツ全体の幅・高さも持つ。
#[derive(Debug, Clone, PartialEq)]
pub struct Layout {
    pub pages: Vec<PageRect>,
    pub width: f32,
    pub height: f32,
}

/// `sizes`(各ページのPDFポイント単位の幅・高さ)を`zoom`倍して縦に積む。
/// ページは、コンテンツ幅と`viewport_width`の大きい方に対して中央揃えにする。
pub fn compute(sizes: &[(f32, f32)], zoom: f32, viewport_width: f32) -> Layout {
    let widest = sizes.iter().map(|s| s.0 * zoom).fold(0.0, f32::max);
    let width = (widest + MARGIN * 2.0).max(viewport_width);
    let mut top = MARGIN;
    let pages = sizes
        .iter()
        .map(|&(w, h)| {
            let (width_px, height_px) = (w * zoom, h * zoom);
            let rect = PageRect {
                left: (width - width_px) / 2.0,
                top,
                width: width_px,
                height: height_px,
            };
            top += height_px + PAGE_GAP;
            rect
        })
        .collect();
    let height = if sizes.is_empty() {
        MARGIN * 2.0
    } else {
        top - PAGE_GAP + MARGIN
    };
    Layout {
        pages,
        width,
        height,
    }
}

impl Layout {
    /// `scroll_top`から`viewport_height`の範囲に少しでもかかるページ番号の範囲。
    pub fn visible_range(&self, scroll_top: f32, viewport_height: f32) -> Range<usize> {
        let bottom = scroll_top + viewport_height;
        let first = self.pages.partition_point(|p| p.bottom() <= scroll_top);
        let last = self.pages.partition_point(|p| p.top < bottom);
        first..last.max(first)
    }

    /// 画面上端の位置`y`に最も近いページ(`y`がページ間の隙間なら直後のページ)。
    pub fn page_at(&self, y: f32) -> usize {
        let ix = self.pages.partition_point(|p| p.bottom() <= y);
        ix.min(self.pages.len().saturating_sub(1))
    }

    /// ページ`index`の上端にスクロールするときのスクロール量。範囲外は最後のページ。
    pub fn scroll_top_of(&self, index: usize) -> f32 {
        self.pages
            .get(index)
            .or(self.pages.last())
            .map_or(0.0, |p| p.top - MARGIN)
    }
}

/// 内容の幅が表示領域より広いとき、内容を水平方向の中央に置くスクロール量。
/// 内容が収まるときは0。
pub fn centered_scroll_x(content_width: f32, viewport_width: f32) -> f32 {
    ((content_width - viewport_width) / 2.0).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIZES: [(f32, f32); 3] = [(100.0, 200.0), (50.0, 100.0), (100.0, 200.0)];

    #[test]
    fn stacks_pages_with_gap_and_margin() {
        let l = compute(&SIZES, 1.0, 0.0);
        assert_eq!(l.pages[0].top, MARGIN);
        assert_eq!(l.pages[1].top, MARGIN + 200.0 + PAGE_GAP);
        assert_eq!(l.height, MARGIN * 2.0 + 500.0 + PAGE_GAP * 2.0);
    }

    #[test]
    fn zoom_scales_pages_but_not_gaps() {
        let l = compute(&SIZES, 2.0, 0.0);
        assert_eq!(l.pages[0].height, 400.0);
        assert_eq!(l.pages[1].top, MARGIN + 400.0 + PAGE_GAP);
        assert_eq!(l.width, 200.0 + MARGIN * 2.0);
    }

    #[test]
    fn narrow_pages_are_centered() {
        let l = compute(&SIZES, 1.0, 0.0);
        assert_eq!(l.pages[1].left, (l.width - 50.0) / 2.0);
        assert_eq!(l.pages[0].left, MARGIN);
    }

    #[test]
    fn content_is_at_least_viewport_wide() {
        let l = compute(&SIZES, 1.0, 1000.0);
        assert_eq!(l.width, 1000.0);
        assert_eq!(l.pages[0].left, 450.0);
    }

    #[test]
    fn empty_document_has_only_margins() {
        let l = compute(&[], 1.0, 0.0);
        assert!(l.pages.is_empty());
        assert_eq!(l.height, MARGIN * 2.0);
        assert_eq!(l.visible_range(0.0, 100.0), 0..0);
        assert_eq!(l.page_at(0.0), 0);
    }

    #[test]
    fn visible_range_includes_partially_visible_pages() {
        let l = compute(&SIZES, 1.0, 0.0);
        // ページ0: 12..212, ページ1: 224..324, ページ2: 336..536
        assert_eq!(l.visible_range(0.0, 100.0), 0..1);
        assert_eq!(l.visible_range(200.0, 30.0), 0..2);
        assert_eq!(l.visible_range(215.0, 5.0), 1..1); // 隙間だけ
        assert_eq!(l.visible_range(300.0, 100.0), 1..3);
        assert_eq!(l.visible_range(9999.0, 100.0), 3..3);
    }

    #[test]
    fn page_at_picks_the_page_under_or_after_y() {
        let l = compute(&SIZES, 1.0, 0.0);
        assert_eq!(l.page_at(0.0), 0);
        assert_eq!(l.page_at(211.0), 0);
        assert_eq!(l.page_at(215.0), 1);
        assert_eq!(l.page_at(9999.0), 2);
    }

    #[test]
    fn scroll_top_of_page() {
        let l = compute(&SIZES, 1.0, 0.0);
        assert_eq!(l.scroll_top_of(0), 0.0);
        assert_eq!(l.scroll_top_of(1), 200.0 + PAGE_GAP);
        assert_eq!(l.scroll_top_of(99), l.scroll_top_of(2));
    }

    #[test]
    fn centered_scroll_splits_the_overflow() {
        assert_eq!(centered_scroll_x(1000.0, 600.0), 200.0);
    }

    #[test]
    fn centered_scroll_is_zero_when_content_fits() {
        assert_eq!(centered_scroll_x(500.0, 600.0), 0.0);
        assert_eq!(centered_scroll_x(600.0, 600.0), 0.0);
    }
}
