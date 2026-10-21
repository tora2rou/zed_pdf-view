//! ページの文字に対する、選択のための操作(位置から文字を探す・範囲の文字列・ハイライト)。

use std::ops::Range;

use crate::{
    links::LinkRect,
    text::{Gap, PageText, TextChar},
};

/// 文字の種類。語の区切りの判定に使う。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Class {
    Space,
    /// 英数字(ひとつづきで1語になる)
    Alnum,
    /// 漢字・かな(ひとつづきで1語になる。句読点は含まない)
    Cjk,
    /// 句読点や記号。1文字ずつが1語
    Punct,
}

fn classify(c: char) -> Class {
    if c.is_whitespace() {
        Class::Space
    } else if is_cjk_letter(c) {
        Class::Cjk
    } else if c.is_alphanumeric() || c == '_' {
        Class::Alnum
    } else {
        Class::Punct
    }
}

/// 漢字・ひらがな・カタカナ(長音符`ー`を含む)。句読点や括弧は含めない。
fn is_cjk_letter(c: char) -> bool {
    matches!(c,
        '\u{3041}'..='\u{3096}'   // ひらがな
        | '\u{30A1}'..='\u{30FA}' // カタカナ
        | '\u{30FC}'              // 長音符
        | '\u{31F0}'..='\u{31FF}' // カタカナ拡張
        | '\u{3400}'..='\u{4DBF}' // 漢字(拡張A)
        | '\u{4E00}'..='\u{9FFF}' // 漢字
        | '\u{F900}'..='\u{FAFF}' // 互換漢字
        | '\u{FF66}'..='\u{FF9F}' // 半角カタカナ
    )
}

impl TextChar {
    fn class(&self) -> Class {
        let mut chars = self.text.chars();
        match (chars.next(), chars.next()) {
            (Some(c), None) => classify(c),
            // 合字など複数文字なら、先頭の文字で決める。空なら空白として扱う。
            (Some(c), Some(_)) => classify(c),
            (None, _) => Class::Space,
        }
    }

    fn center(&self) -> (f32, f32) {
        (
            self.rect.x + self.rect.width / 2.0,
            self.rect.y + self.rect.height / 2.0,
        )
    }

    /// 点から、この文字の箱までの距離(箱の中なら0)。
    fn distance_to(&self, x: f32, y: f32) -> f32 {
        let dx = (self.rect.x - x)
            .max(0.0)
            .max(x - (self.rect.x + self.rect.width));
        let dy = (self.rect.y - y)
            .max(0.0)
            .max(y - (self.rect.y + self.rect.height));
        dx.hypot(dy)
    }
}

impl PageText {
    /// 点`(x, y)`(表示座標)に最も近い、文字と文字の間(キャレット位置、0〜文字数)。
    /// 最も近い文字を探し、その文字の中心より進む向きの前か後ろかで、手前の位置か次の位置かを決める。
    /// 文字が1つも無ければ`None`。
    pub fn caret_at(&self, x: f32, y: f32) -> Option<usize> {
        let (index, nearest) = self.chars.iter().enumerate().min_by(|(_, a), (_, b)| {
            let key = |c: &TextChar| {
                let (cx, cy) = c.center();
                (c.distance_to(x, y), (cx - x).hypot(cy - y))
            };
            key(a)
                .partial_cmp(&key(b))
                .unwrap_or(std::cmp::Ordering::Equal)
        })?;
        let (cx, cy) = nearest.center();
        let along = (x - cx) * nearest.dir.0 + (y - cy) * nearest.dir.1;
        Some(if along > 0.0 { index + 1 } else { index })
    }

    /// `range`の文字を、読める文字列にする。語の区切りには空白、行の区切りには改行を入れる。
    pub fn string_in(&self, range: Range<usize>) -> String {
        let range = self.clamp(range);
        let mut out = String::new();
        for (offset, c) in self.chars[range.clone()].iter().enumerate() {
            let is_space = c.class() == Class::Space;
            if offset > 0 && !is_space {
                match c.gap {
                    Gap::None => {}
                    // 直前が空白の文字なら、区切りの空白を重ねない
                    Gap::Space if !out.ends_with([' ', '\n']) => out.push(' '),
                    Gap::Line if !out.ends_with('\n') => out.push('\n'),
                    _ => {}
                }
            }
            if is_space && (out.is_empty() || out.ends_with([' ', '\n'])) {
                continue;
            }
            out.push_str(&c.text);
        }
        out
    }

    /// `range`の文字を覆うハイライトの矩形。同じ行の続きは、1つにまとめる。
    pub fn selection_rects(&self, range: Range<usize>) -> Vec<LinkRect> {
        let range = self.clamp(range);
        let mut rects: Vec<LinkRect> = Vec::new();
        let mut current: Option<LinkRect> = None;
        for (offset, c) in self.chars[range].iter().enumerate() {
            if offset > 0 && c.gap == Gap::Line {
                rects.extend(current.take());
            }
            current = Some(match current {
                Some(r) => union(r, c.rect),
                None => c.rect,
            });
        }
        rects.extend(current);
        rects
    }

    /// `index`の文字を含む語の範囲。範囲外なら空。
    pub fn word_range(&self, index: usize) -> Range<usize> {
        let Some(c) = self.chars.get(index) else {
            return index.min(self.chars.len())..index.min(self.chars.len());
        };
        // 空白、記号は1文字だけ(空白の連続はまとめない)
        if matches!(c.class(), Class::Space | Class::Punct) {
            return index..index + 1;
        }
        let same_word = |a: usize, b: usize| {
            // aの次がb(a < b)のとき、同じ語か
            self.chars[b].gap == Gap::None && self.chars[a].class() == self.chars[b].class()
        };
        let mut start = index;
        while start > 0 && same_word(start - 1, start) {
            start -= 1;
        }
        let mut end = index + 1;
        while end < self.chars.len() && same_word(end - 1, end) {
            end += 1;
        }
        start..end
    }

    /// `index`の文字を含む行の範囲。範囲外なら空。
    pub fn line_range(&self, index: usize) -> Range<usize> {
        if index >= self.chars.len() {
            let n = self.chars.len();
            return index.min(n)..index.min(n);
        }
        let mut start = index;
        while start > 0 && self.chars[start].gap != Gap::Line {
            start -= 1;
        }
        let mut end = index + 1;
        while end < self.chars.len() && self.chars[end].gap != Gap::Line {
            end += 1;
        }
        start..end
    }

    /// 範囲を、文字数に収める(逆順や範囲外でも安全に使えるように)。
    fn clamp(&self, range: Range<usize>) -> Range<usize> {
        let n = self.chars.len();
        let start = range.start.min(n);
        start..range.end.clamp(start, n)
    }
}

fn union(a: LinkRect, b: LinkRect) -> LinkRect {
    let (x0, y0) = (a.x.min(b.x), a.y.min(b.y));
    let (x1, y1) = (
        (a.x + a.width).max(b.x + b.width),
        (a.y + a.height).max(b.y + b.height),
    );
    LinkRect {
        x: x0,
        y: y0,
        width: x1 - x0,
        height: y1 - y0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 行ごとの文字列から、10x12の箱を横に並べたページを作る。行は下に15ずつ。
    fn page(lines: &[&str]) -> PageText {
        let mut chars = Vec::new();
        for (row, line) in lines.iter().enumerate() {
            for (col, ch) in line.chars().enumerate() {
                chars.push(TextChar {
                    text: ch.to_string(),
                    rect: LinkRect {
                        x: col as f32 * 10.0,
                        y: row as f32 * 15.0,
                        width: 10.0,
                        height: 12.0,
                    },
                    gap: if col == 0 { Gap::Line } else { Gap::None },
                    dir: (1.0, 0.0),
                });
            }
        }
        PageText { chars }
    }

    #[test]
    fn caret_goes_before_or_after_a_char_by_which_half_is_hit() {
        let text = page(&["abc"]);
        // 'b' は x 10..20。左半分なら 'b' の手前(1)、右半分なら次(2)
        assert_eq!(text.caret_at(12.0, 6.0), Some(1));
        assert_eq!(text.caret_at(18.0, 6.0), Some(2));
        // 左端の外は先頭、右端の外は末尾
        assert_eq!(text.caret_at(-50.0, 6.0), Some(0));
        assert_eq!(text.caret_at(500.0, 6.0), Some(3));
    }

    #[test]
    fn caret_picks_the_nearest_line() {
        let text = page(&["abc", "def"]);
        // 2行目(y 15..27)の 'e'(3+1=4番目)の右半分
        assert_eq!(text.caret_at(18.0, 20.0), Some(5));
        // 行の間のすき間(y=13.5)は、近い方の行
        assert_eq!(text.caret_at(12.0, 13.0), Some(1));
        assert_eq!(text.caret_at(12.0, 14.5), Some(4));
    }

    #[test]
    fn caret_on_empty_page_is_none() {
        assert_eq!(PageText::default().caret_at(0.0, 0.0), None);
    }

    #[test]
    fn caret_follows_a_text_direction() {
        // 下向きに進む文字(縦書きや、90度回転したページ)
        let mut text = page(&["a"]);
        text.chars[0].dir = (0.0, 1.0);
        let (cx, cy) = text.chars[0].center();
        assert_eq!(text.caret_at(cx, cy - 3.0), Some(0)); // 進む向きの手前
        assert_eq!(text.caret_at(cx, cy + 3.0), Some(1)); // 進む向きの先
        // 逆向き(右から左)
        text.chars[0].dir = (-1.0, 0.0);
        assert_eq!(text.caret_at(cx + 3.0, cy), Some(0));
        assert_eq!(text.caret_at(cx - 3.0, cy), Some(1));
    }

    #[test]
    fn string_in_inserts_newlines_between_lines() {
        let text = page(&["ab", "cd"]);
        assert_eq!(text.string_in(0..4), "ab\ncd");
        assert_eq!(text.string_in(1..3), "b\nc");
        assert_eq!(text.string_in(0..0), "");
    }

    #[test]
    fn string_in_inserts_spaces_at_word_gaps_without_doubling() {
        let mut text = page(&["ab cd"]);
        // "ab", 空白の文字、"cd"。空白文字の後ろは区切りなしだが、前の語と空白文字の間が空いている
        text.chars[3].gap = Gap::Space; // 'c' の前に区切り
        assert_eq!(text.string_in(0..5), "ab cd");
        // 空白の文字が無く、区切りだけがある場合も空白になる
        let mut text = page(&["abcd"]);
        text.chars[2].gap = Gap::Space;
        assert_eq!(text.string_in(0..4), "ab cd");
    }

    #[test]
    fn string_in_is_safe_for_bad_ranges() {
        let text = page(&["ab"]);
        assert_eq!(text.string_in(5..9), "");
        assert_eq!(text.string_in(1..100), "b");
        #[allow(clippy::reversed_empty_ranges)]
        let reversed = 2..1;
        assert_eq!(text.string_in(reversed), "");
    }

    #[test]
    fn selection_rects_merge_each_line() {
        let text = page(&["abc", "de"]);
        let rects = text.selection_rects(1..5);
        // 1行目の 'b','c' と、2行目の 'd','e'
        assert_eq!(
            rects,
            [
                LinkRect {
                    x: 10.0,
                    y: 0.0,
                    width: 20.0,
                    height: 12.0
                },
                LinkRect {
                    x: 0.0,
                    y: 15.0,
                    width: 20.0,
                    height: 12.0
                },
            ]
        );
        assert!(text.selection_rects(2..2).is_empty());
    }

    #[test]
    fn word_range_covers_a_latin_word() {
        let mut text = page(&["hello world"]);
        text.chars[6].gap = Gap::None;
        assert_eq!(text.word_range(1), 0..5); // "hello"
        assert_eq!(text.word_range(8), 6..11); // "world"
        assert_eq!(text.word_range(5), 5..6); // 空白は1文字だけ
    }

    #[test]
    fn word_range_splits_japanese_from_latin_and_punctuation() {
        let text = page(&["日本語ABC、次"]);
        assert_eq!(text.word_range(1), 0..3); // 日本語
        assert_eq!(text.word_range(4), 3..6); // ABC
        assert_eq!(text.word_range(6), 6..7); // 、
        assert_eq!(text.word_range(7), 7..8); // 次
    }

    #[test]
    fn word_range_stops_at_gaps() {
        let mut text = page(&["abcd"]);
        text.chars[2].gap = Gap::Space;
        assert_eq!(text.word_range(0), 0..2);
        assert_eq!(text.word_range(3), 2..4);
    }

    #[test]
    fn line_range_covers_the_whole_line() {
        let text = page(&["abc", "de"]);
        assert_eq!(text.line_range(1), 0..3);
        assert_eq!(text.line_range(4), 3..5);
        assert_eq!(text.line_range(99), 5..5);
    }
}
