//! テキスト選択の範囲のモデル。GPUIに依存しない純粋なデータと計算。

use std::ops::{Range, RangeInclusive};

/// 文字と文字の間の位置(キャレット)。`index`はそのページの何文字目の手前か(文字数なら末尾)。
/// 並びは、ページ番号、続いてページ内の位置の順。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Caret {
    pub page: usize,
    pub index: usize,
}

/// ドラッグで選んだ範囲。`anchor`が押した位置、`focus`がいま動かしている側。
/// 複数のページにまたがってよい。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    pub anchor: Caret,
    pub focus: Caret,
}

impl Selection {
    /// 文書全体。末尾は文字数が分からない(未取り出しのページもある)ので、`usize::MAX`で表す。
    pub fn all(page_count: usize) -> Option<Self> {
        let last = page_count.checked_sub(1)?;
        Some(Self {
            anchor: Caret { page: 0, index: 0 },
            focus: Caret {
                page: last,
                index: usize::MAX,
            },
        })
    }

    /// 小さい方、大きい方の順に並べた両端。
    pub fn ordered(&self) -> (Caret, Caret) {
        if self.anchor <= self.focus {
            (self.anchor, self.focus)
        } else {
            (self.focus, self.anchor)
        }
    }

    /// 何も選んでいない(両端が同じ位置)か。
    pub fn is_empty(&self) -> bool {
        self.anchor == self.focus
    }

    /// 選択にかかるページの範囲。
    pub fn pages(&self) -> RangeInclusive<usize> {
        let (start, end) = self.ordered();
        start.page..=end.page
    }

    /// ページ`page`(文字数`len`)のうち、選ばれている文字の範囲。かかっていなければ`None`。
    /// 先頭のページは開始位置から、途中のページは全部、最後のページは終了位置まで。
    pub fn range_on_page(&self, page: usize, len: usize) -> Option<Range<usize>> {
        let (start, end) = self.ordered();
        if page < start.page || page > end.page {
            return None;
        }
        let from = if page == start.page {
            start.index.min(len)
        } else {
            0
        };
        let to = if page == end.page {
            end.index.min(len)
        } else {
            len
        };
        (from < to).then_some(from..to)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn caret(page: usize, index: usize) -> Caret {
        Caret { page, index }
    }

    fn selection(a: (usize, usize), f: (usize, usize)) -> Selection {
        Selection {
            anchor: caret(a.0, a.1),
            focus: caret(f.0, f.1),
        }
    }

    #[test]
    fn carets_order_by_page_then_index() {
        assert!(caret(0, 99) < caret(1, 0));
        assert!(caret(2, 3) < caret(2, 4));
    }

    #[test]
    fn selection_works_in_either_drag_direction() {
        let forward = selection((1, 5), (2, 3));
        let backward = selection((2, 3), (1, 5));
        assert_eq!(forward.ordered(), backward.ordered());
        assert_eq!(forward.ordered(), (caret(1, 5), caret(2, 3)));
        assert_eq!(forward.pages(), 1..=2);
    }

    #[test]
    fn same_position_is_empty() {
        assert!(selection((1, 4), (1, 4)).is_empty());
        assert!(!selection((1, 4), (1, 5)).is_empty());
    }

    #[test]
    fn single_page_selection_is_the_index_range() {
        let s = selection((1, 3), (1, 8));
        assert_eq!(s.range_on_page(1, 100), Some(3..8));
        assert_eq!(s.range_on_page(0, 100), None);
        assert_eq!(s.range_on_page(2, 100), None);
        // 逆向きでも同じ
        assert_eq!(selection((1, 8), (1, 3)).range_on_page(1, 100), Some(3..8));
    }

    #[test]
    fn multi_page_selection_covers_the_middle_pages_fully() {
        let s = selection((1, 30), (3, 12));
        assert_eq!(s.range_on_page(1, 100), Some(30..100)); // 先頭のページ: 開始位置から末尾まで
        assert_eq!(s.range_on_page(2, 80), Some(0..80)); // 途中のページ: 全部
        assert_eq!(s.range_on_page(3, 50), Some(0..12)); // 最後のページ: 先頭から終了位置まで
        assert_eq!(s.range_on_page(4, 50), None);
    }

    #[test]
    fn positions_beyond_the_text_are_clamped() {
        let s = selection((0, 5), (0, 1000));
        assert_eq!(s.range_on_page(0, 20), Some(5..20));
        // 文字の無いページ(len 0)は、何も選ばれない
        assert_eq!(selection((0, 0), (2, 0)).range_on_page(1, 0), None);
    }

    #[test]
    fn select_all_spans_the_whole_document() {
        let s = Selection::all(3).unwrap();
        assert_eq!(s.pages(), 0..=2);
        assert_eq!(s.range_on_page(0, 40), Some(0..40));
        assert_eq!(s.range_on_page(1, 25), Some(0..25));
        assert_eq!(s.range_on_page(2, 10), Some(0..10));
        assert_eq!(Selection::all(0), None);
    }
}
