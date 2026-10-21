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

    /// 内容上の点`(x, y)`にあるページと、そのページ内での位置(ページの左上が原点)。
    /// ページの外(余白や隙間)なら`None`。
    pub fn hit_page(&self, x: f32, y: f32) -> Option<(usize, f32, f32)> {
        let index = self.pages.partition_point(|p| p.bottom() <= y);
        let page = self.pages.get(index)?;
        let inside = y >= page.top && x >= page.left && x < page.left + page.width;
        inside.then_some((index, x - page.left, y - page.top))
    }

    /// 内容上の点`(x, y)`に最も近いページと、そのページ内での位置(ページの外なら、ページの縁に丸める)。
    /// ドラッグの途中でマウスがページの外(余白や隙間、別のページの上)に出ても、選択を続けられるようにする。
    /// ページが1つも無ければ`None`。
    pub fn nearest_page_point(&self, x: f32, y: f32) -> Option<(usize, f32, f32)> {
        if self.pages.is_empty() {
            return None;
        }
        // yに最も近い(含んでいれば距離0の)ページ
        let index = self
            .pages
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| {
                let distance = |p: &PageRect| (p.top - y).max(0.0).max(y - p.bottom());
                distance(a)
                    .partial_cmp(&distance(b))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(i, _)| i)?;
        let page = self.pages[index];
        Some((
            index,
            (x - page.left).clamp(0.0, page.width),
            (y - page.top).clamp(0.0, page.height),
        ))
    }

    /// `scroll_top`を、実際にスクロールできる範囲(0〜内容の高さ−表示の高さ)に収める。
    ///
    /// GPUIは、勢いよくスクロールした直後に1フレームだけ範囲外の位置を返すことがある
    /// (次の描画で範囲内に戻る)。その位置から「見えているページ」を求めると空になり、
    /// 保持している画像を全部捨てて描き直してしまう(画面が点滅する)ので、先に収める。
    pub fn clamp_scroll_top(&self, scroll_top: f32, viewport_height: f32) -> f32 {
        scroll_top.clamp(0.0, (self.height - viewport_height).max(0.0))
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

    #[test]
    fn clamp_scroll_top_keeps_the_view_inside_the_content() {
        let l = compute(&SIZES, 1.0, 0.0); // 内容の高さ = 24 + 500 + 24 = 548
        assert_eq!(l.height, 548.0);
        assert_eq!(l.clamp_scroll_top(100.0, 200.0), 100.0);
        assert_eq!(l.clamp_scroll_top(-30.0, 200.0), 0.0); // 先頭より上
        assert_eq!(l.clamp_scroll_top(9999.0, 200.0), 348.0); // 末尾より下
        assert_eq!(l.clamp_scroll_top(50.0, 9999.0), 0.0); // 内容が画面に収まる
    }

    #[test]
    fn overscrolled_position_still_sees_the_last_pages() {
        let l = compute(&SIZES, 1.0, 0.0);
        // 補正しないと「何も見えない」になる位置でも、補正後は末尾のページが見える
        assert_eq!(l.visible_range(9999.0, 200.0), 3..3);
        let clamped = l.clamp_scroll_top(9999.0, 200.0);
        assert_eq!(l.visible_range(clamped, 200.0), 2..3);
    }

    #[test]
    fn hit_page_returns_page_local_coordinates() {
        let l = compute(&SIZES, 1.0, 0.0);
        // ページ0は(12, 12)から 100x200
        assert_eq!(l.hit_page(12.0, 12.0), Some((0, 0.0, 0.0)));
        assert_eq!(l.hit_page(50.0, 112.0), Some((0, 38.0, 100.0)));
        // ページ1は幅50で中央揃え: left = (124 - 50) / 2 = 37、top = 224
        assert_eq!(l.hit_page(40.0, 230.0), Some((1, 3.0, 6.0)));
    }

    #[test]
    fn hit_page_misses_margins_and_gaps() {
        let l = compute(&SIZES, 1.0, 0.0);
        assert_eq!(l.hit_page(5.0, 50.0), None); // 左の余白
        assert_eq!(l.hit_page(50.0, 5.0), None); // 上の余白
        assert_eq!(l.hit_page(50.0, 215.0), None); // ページ間の隙間
        assert_eq!(l.hit_page(20.0, 230.0), None); // 幅の狭いページの左の空き
        assert_eq!(l.hit_page(50.0, 9999.0), None); // 末尾より下
        assert_eq!(compute(&[], 1.0, 0.0).hit_page(10.0, 10.0), None);
    }

    #[test]
    fn nearest_page_point_is_exact_inside_a_page() {
        let l = compute(&SIZES, 1.0, 0.0);
        assert_eq!(l.nearest_page_point(50.0, 112.0), Some((0, 38.0, 100.0)));
    }

    #[test]
    fn nearest_page_point_clamps_outside_the_page() {
        let l = compute(&SIZES, 1.0, 0.0);
        // 左の余白(x=5)→ページ0の左端、上の余白→上端
        assert_eq!(l.nearest_page_point(5.0, 50.0), Some((0, 0.0, 38.0)));
        assert_eq!(l.nearest_page_point(50.0, 2.0), Some((0, 38.0, 0.0)));
        // 末尾より下は、最後のページの下端
        assert_eq!(l.nearest_page_point(50.0, 9999.0), Some((2, 38.0, 200.0)));
    }

    #[test]
    fn nearest_page_point_in_a_gap_picks_the_closer_page() {
        let l = compute(&SIZES, 1.0, 0.0);
        // ページ0の下端は212、ページ1の上端は224。隙間(212〜224)の中で近い方を選ぶ
        assert_eq!(l.nearest_page_point(50.0, 214.0).unwrap().0, 0);
        assert_eq!(l.nearest_page_point(50.0, 222.0).unwrap().0, 1);
    }

    #[test]
    fn nearest_page_point_without_pages_is_none() {
        assert_eq!(compute(&[], 1.0, 0.0).nearest_page_point(0.0, 0.0), None);
    }
}
