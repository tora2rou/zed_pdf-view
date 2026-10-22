//! OCR(Tesseract)の出力を、ページの文字(`PageText`)に変換する。

use std::{
    io::{ErrorKind, Write as _},
    process::{Command, Stdio},
    thread,
};

use crate::{
    error::PdfError,
    links::LinkRect,
    render::RenderedPage,
    text::{Gap, PageText, TextChar},
};

/// 外部コマンドとして呼ぶTesseractの名前。
const TESSERACT: &str = "tesseract";
/// 使いたい言語。入っているものだけを使う。日本語の文書に英単語が混ざるので、両方を渡す。
const PREFERRED_LANGUAGES: [&str; 2] = ["jpn", "eng"];
/// 言語を上書きする環境変数(`jpn+eng`のようにTesseractの書式で書く)。
const LANGUAGE_ENV: &str = "PDF_VIEW_OCR_LANG";

/// TSVの単語の行の`level`。
const WORD_LEVEL: u32 = 5;
/// これ未満の確信度の単語は捨てる。-1は単語以外の行。日本語は正しく読めても0付近の確信度が付くことが
/// あるので、0は捨てない。
/// TSVの列の数。最後の列(文字列)に、区切りと同じタブが入ることはない。
const COLUMNS: usize = 12;

/// OCRする画像の解像度(dpi)の目安。小さい文字は、これくらいないと読めない。
pub const OCR_DPI: f32 = 300.0;

/// Tesseractに渡す言語。環境変数があればそれ、なければ入っている言語のうち使いたいもの。
fn language(program: &str) -> Result<String, PdfError> {
    if let Ok(lang) = std::env::var(LANGUAGE_ENV)
        && !lang.trim().is_empty()
    {
        return Ok(lang.trim().to_string());
    }
    let output = Command::new(program)
        .arg("--list-langs")
        .output()
        .map_err(|e| spawn_error(program, &e))?;
    // 言語の一覧は、バージョンによって標準出力と標準エラー出力のどちらかに出る。
    let listing = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let languages = pick_languages(&listing);
    if languages.is_empty() {
        return Err(PdfError::Ocr(format!(
            "Tesseractの言語データ({})が入っていません",
            PREFERRED_LANGUAGES.join(", ")
        )));
    }
    Ok(languages.join("+"))
}

/// `tesseract --list-langs`の出力から、使いたい言語のうち入っているものを選ぶ。
fn pick_languages(listing: &str) -> Vec<&'static str> {
    PREFERRED_LANGUAGES
        .into_iter()
        .filter(|wanted| listing.lines().any(|line| line.trim() == *wanted))
        .collect()
}

fn spawn_error(program: &str, error: &std::io::Error) -> PdfError {
    if error.kind() == ErrorKind::NotFound {
        PdfError::Ocr(format!(
            "{program}コマンドが見つかりません。Tesseractをインストールしてください"
        ))
    } else {
        PdfError::Ocr(format!("{program}を起動できません: {error}"))
    }
}

/// RGBAの画像を、Tesseract(Leptonica)が標準入力から読めるPPM(P6)にする。
fn encode_ppm(page: &RenderedPage) -> Vec<u8> {
    let mut out = format!("P6\n{} {}\n255\n", page.width, page.height).into_bytes();
    out.reserve(page.rgba.len() / 4 * 3);
    for pixel in page.rgba.as_chunks::<4>().0 {
        out.extend_from_slice(&pixel[..3]);
    }
    out
}

/// `page`の画像をTesseract(`program`)でOCRして、TSVを返す。
fn recognize(program: &str, page: &RenderedPage) -> Result<String, PdfError> {
    let lang = language(program)?;
    let mut child = Command::new(program)
        .args(["stdin", "stdout", "-l", &lang, "tsv"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| spawn_error(program, &e))?;
    let mut stdin = child.stdin.take().expect("stdinはpipedにした");
    let image = encode_ppm(page);
    // 出力のパイプが詰まって止まらないよう、書き込みは別スレッドで行う。
    let writer = thread::spawn(move || stdin.write_all(&image));
    let output = child
        .wait_with_output()
        .map_err(|e| PdfError::Ocr(e.to_string()))?;
    // Tesseractが先に終わったときの書き込み失敗は、終了状態の方で報告する。
    let _ = writer.join();
    if !output.status.success() {
        return Err(PdfError::Ocr(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// `page`(`scale`は画素 / ポイント)をOCRして、ページの文字にする。
pub fn recognize_page(page: &RenderedPage, scale: f32) -> Result<PageText, PdfError> {
    Ok(page_text_from_tsv(&recognize(TESSERACT, page)?, scale))
}

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
        let adjacent_cjk = chars.last().zip(word.text.chars().next()).is_some_and(
            |(last, next): (&TextChar, char)| {
                last.text.chars().next().is_some_and(is_cjk) && is_cjk(next)
            },
        );
        let gap = match previous_line {
            // Tesseractは日本語を1文字〜数文字ずつの単語に分けるが、日本語は語の間に空白を書かない。
            Some(previous) if previous == line_id && adjacent_cjk => Gap::None,
            Some(previous) if previous == line_id => Gap::Space,
            _ => Gap::Line,
        };
        previous_line = Some(line_id);
        push_word(&mut chars, &word, gap, scale);
    }
    PageText { chars }
}

/// 日本語(かな・漢字・全角)の文字か。
fn is_cjk(c: char) -> bool {
    matches!(
        c as u32,
        0x3000..=0x30FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF | 0xFF00..=0xFFEF
    )
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
        if text.is_empty() || confidence < 0.0 {
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

    fn page(width: u32, height: u32) -> RenderedPage {
        RenderedPage {
            width,
            height,
            rgba: vec![10; (width * height * 4) as usize],
        }
    }

    #[test]
    fn ppm_drops_the_alpha_channel() {
        let mut image = page(2, 1);
        image.rgba = vec![1, 2, 3, 255, 4, 5, 6, 255];
        let mut expected = b"P6\n2 1\n255\n".to_vec();
        expected.extend([1, 2, 3, 4, 5, 6]);
        assert_eq!(encode_ppm(&image), expected);
    }

    #[test]
    fn picks_only_installed_preferred_languages() {
        let listing = "List of available languages (3):\neng\nfra\njpn\n";
        assert_eq!(pick_languages(listing), ["jpn", "eng"]);
        assert_eq!(
            pick_languages("List of available languages (1):\nfra\n"),
            [] as [&str; 0]
        );
        // "jpn_vert"のような別の言語データを、jpnと取り違えない。
        assert_eq!(pick_languages("jpn_vert\n"), [] as [&str; 0]);
    }

    #[test]
    fn a_missing_tesseract_is_reported_clearly() {
        let error = recognize("pdf-view-no-such-command", &page(1, 1)).unwrap_err();
        assert!(
            matches!(&error, PdfError::Ocr(m) if m.contains("見つかりません")),
            "{error}"
        );
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
                "5\t1\t1\t1\t1\t1\t0\t0\t20\t10\t-1\tnoise",
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
    fn a_zero_confidence_word_is_kept() {
        let text = page_text_from_tsv(&tsv(&["5\t1\t1\t1\t1\t1\t0\t0\t20\t10\t0.0\t認識"]), 1.0);
        assert_eq!(text.to_plain_string(), "認識");
    }

    /// 日本語の単語どうしは空白なしでつなぐが、英単語との間や英単語どうしの間には空白を入れる。
    #[test]
    fn spaces_are_kept_only_outside_japanese() {
        let text = page_text_from_tsv(
            &tsv(&[
                "5\t1\t1\t1\t1\t1\t0\t0\t10\t10\t90\tOCR",
                "5\t1\t1\t1\t1\t2\t20\t0\t10\t10\t90\t日",
                "5\t1\t1\t1\t1\t3\t40\t0\t10\t10\t90\t本",
                "5\t1\t1\t1\t1\t4\t60\t0\t10\t10\t90\tabc",
            ]),
            1.0,
        );
        assert_eq!(text.to_plain_string(), "OCR 日本 abc");
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
