//! OCR(Tesseract)の出力を、ページの文字(`PageText`)に変換する。

use crate::{
    links::LinkRect,
    text::{Gap, PageText, TextChar},
};

/// TSVの単語の行の`level`。
const WORD_LEVEL: u32 = 5;
/// これ未満の確信度の単語は捨てる(Tesseractは、ゴミを低い確信度の単語として返すことがある)。
const MIN_CONFIDENCE: f32 = 10.0;
/// TSVの列の数。最後の列(文字列)に、区切りと同じタブが入ることはない。
const COLUMNS: usize = 12;

/// Tesseractが出力するTSVから、ページの文字を作る。
///
/// 座標は画像の画素なので、`scale`(画素 / ポイント)で割って、表示座標(倍率1.0、ポイント)に直す。
/// 単語は文字の数で等分して、1文字ずつの箱にする(日本語は全角なので、ほぼ合う)。
pub fn page_text_from_tsv(tsv: &str, scale: f32) -> PageText {
    let mut chars = Vec::new();
    // 直前の単語の(block, paragraph, line)。同じなら同じ行。
    let mut previous_line = None;
    for line in tsv.lines().skip(1) {
        let Some(word) = Word::parse(line) else {
            continue;
        };
        let line_id = word.line_id;
        let gap = match previous_line {
            Some(previous) if previous == line_id => Gap::Space,
            _ => Gap::Line,
        };
        previous_line = Some(line_id);
        push_word(&mut chars, &word, gap, scale);
    }
    PageText { chars }
}

fn push_word(chars: &mut Vec<TextChar>, word: &Word<'_>, first_gap: Gap, scale: f32) {
    let count = word.text.chars().count() as f32;
    let char_width = word.width / count / scale;
    for (i, c) in word.text.chars().enumerate() {
        chars.push(TextChar {
            text: c.to_string(),
            rect: LinkRect {
                x: (word.left / scale) + char_width * i as f32,
                y: word.top / scale,
                width: char_width,
                height: word.height / scale,
            },
            gap: if i == 0 { first_gap } else { Gap::None },
            dir: (1.0, 0.0),
        });
    }
}

/// TSVの単語の行。
struct Word<'a> {
    line_id: (u32, u32, u32),
    left: f32,
    top: f32,
    width: f32,
    height: f32,
    text: &'a str,
}

impl<'a> Word<'a> {
    /// 単語の行を読む。単語以外の行(ページ・段落など)や、確信度が低い・空の行は`None`。
    fn parse(line: &'a str) -> Option<Self> {
        let fields: Vec<&str> = line.splitn(COLUMNS, '\t').collect();
        if fields.len() < COLUMNS || fields[0].parse::<u32>().ok()? != WORD_LEVEL {
            return None;
        }
        let text = fields[11].trim();
        let confidence: f32 = fields[10].parse().ok()?;
        if text.is_empty() || confidence < MIN_CONFIDENCE {
            return None;
        }
        let (width, height): (f32, f32) = (fields[8].parse().ok()?, fields[9].parse().ok()?);
        if width <= 0.0 || height <= 0.0 {
            return None;
        }
        Some(Self {
            line_id: (
                fields[2].parse().ok()?,
                fields[3].parse().ok()?,
                fields[4].parse().ok()?,
            ),
            left: fields[6].parse().ok()?,
            top: fields[7].parse().ok()?,
            width,
            height,
            text,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEADER: &str = "level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext";

    fn tsv(rows: &[&str]) -> String {
        std::iter::once(HEADER)
            .chain(rows.iter().copied())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn words_become_boxes_in_points() {
        // 2倍の画像。画素(100, 40)、200x60の単語 "ab" → ポイントでは(50, 20)、100x30
        let text = page_text_from_tsv(&tsv(&["5\t1\t1\t1\t1\t1\t100\t40\t200\t60\t95\tab"]), 2.0);
        assert_eq!(text.to_plain_string(), "ab");
        let a = &text.chars[0].rect;
        assert_eq!((a.x, a.y, a.width, a.height), (50.0, 20.0, 50.0, 30.0));
        assert_eq!(text.chars[1].rect.x, 100.0);
    }

    #[test]
    fn gaps_follow_the_line_structure() {
        let text = page_text_from_tsv(
            &tsv(&[
                "5\t1\t1\t1\t1\t1\t0\t0\t20\t10\t90\tab",
                "5\t1\t1\t1\t1\t2\t30\t0\t20\t10\t90\tcd",
                "5\t1\t1\t1\t2\t1\t0\t20\t20\t10\t90\tef",
            ]),
            1.0,
        );
        let gaps: Vec<Gap> = text.chars.iter().map(|c| c.gap).collect();
        assert_eq!(
            gaps,
            [
                Gap::Line,
                Gap::None,
                Gap::Space,
                Gap::None,
                Gap::Line,
                Gap::None
            ]
        );
        assert_eq!(text.to_plain_string(), "ab cd\nef");
    }

    #[test]
    fn non_words_and_junk_are_skipped() {
        let text = page_text_from_tsv(
            &tsv(&[
                "1\t1\t0\t0\t0\t0\t0\t0\t500\t500\t-1\t",
                "4\t1\t1\t1\t1\t0\t0\t0\t100\t10\t-1\t",
                "5\t1\t1\t1\t1\t1\t0\t0\t20\t10\t3\tnoise",
                "5\t1\t1\t1\t1\t2\t0\t0\t20\t10\t90\t   ",
                "5\t1\t1\t1\t1\t3\t0\t0\t20\t10\t90\tok",
            ]),
            1.0,
        );
        assert_eq!(text.to_plain_string(), "ok");
    }

    #[test]
    fn japanese_words_split_per_character() {
        let text = page_text_from_tsv(&tsv(&["5\t1\t1\t1\t1\t1\t10\t0\t90\t30\t90\t日本語"]), 1.0);
        assert_eq!(text.chars.len(), 3);
        assert_eq!(text.chars[1].rect.x, 40.0);
        assert_eq!(text.chars[1].rect.width, 30.0);
    }

    #[test]
    fn empty_or_garbage_input_gives_no_text() {
        assert!(page_text_from_tsv("", 1.0).chars.is_empty());
        assert!(
            page_text_from_tsv("garbage\nnot\ttsv", 1.0)
                .chars
                .is_empty()
        );
    }
}
