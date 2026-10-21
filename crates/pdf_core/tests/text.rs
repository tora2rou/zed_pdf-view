use pdf_core::{Gap, PdfDocument, PdfError};

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/with_text.pdf");

#[test]
fn extracts_text_in_reading_order_with_line_breaks() {
    let doc = PdfDocument::open(FIXTURE).unwrap();
    assert_eq!(
        doc.page_text(0).unwrap().to_plain_string(),
        "See https://example.com/a/doc.pdf for details.\n\
         Also (http://example.org/x?y=1), and www.example.net.\n\
         No link here: example dot com\n\
         Mail: mailto:me@example.com ok"
    );
}

#[test]
fn page_without_text_is_empty() {
    let doc = PdfDocument::open(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/two_pages.pdf"
    ))
    .unwrap();
    assert!(doc.page_text(0).unwrap().chars.is_empty());
}

#[test]
fn out_of_range_page_is_an_error() {
    let doc = PdfDocument::open(FIXTURE).unwrap();
    assert!(matches!(
        doc.page_text(9),
        Err(PdfError::PageOutOfRange { index: 9, count: 2 })
    ));
}

#[test]
fn char_boxes_sit_on_the_rendered_line() {
    let doc = PdfDocument::open(FIXTURE).unwrap();
    let text = doc.page_text(0).unwrap();
    // 1行目の先頭の文字 "S": 12ptの文字が x=20、ベースライン y=170(PDF) → 表示では y=30
    let first = &text.chars[0];
    assert_eq!(first.text, "S");
    assert_eq!(first.gap, Gap::Line);
    assert!((first.rect.x - 20.0).abs() < 0.5, "{:?}", first.rect);
    // 箱は、ベースライン(表示のy=30)をまたぐ
    assert!(first.rect.y < 30.0 && first.rect.y + first.rect.height > 30.0);
    // 文字の高さは、12pt のおよそ1em
    assert!((first.rect.height - 12.0).abs() < 2.0, "{:?}", first.rect);
}

#[test]
fn text_is_cached_per_page() {
    let doc = PdfDocument::open(FIXTURE).unwrap();
    let first = doc.page_text(0).unwrap() as *const _;
    assert_eq!(first, doc.page_text(0).unwrap() as *const _);
}

/// 90度回転したページでも、文字は読める順に取れ、箱は回転後の表示に合う。
#[test]
fn rotated_page_text_and_boxes() {
    let doc = PdfDocument::open(FIXTURE).unwrap();
    let text = doc.page_text(1).unwrap();
    assert_eq!(
        text.to_plain_string(),
        "Rotated https://example.com/r.pdf here"
    );
    // 回転後の表示は 200x400。文字は縦に並ぶので、最初の文字より最後の文字の方が下にある
    let (first, last) = (&text.chars[0].rect, &text.chars.last().unwrap().rect);
    assert!(last.y > first.y + 100.0, "first {first:?} last {last:?}");
    assert!(
        (last.x - first.x).abs() < 5.0,
        "first {first:?} last {last:?}"
    );
}

/// 取り出した文字の位置で、位置→文字(キャレット)の変換が合う。
#[test]
fn caret_at_a_position_finds_the_right_character() {
    let doc = PdfDocument::open(FIXTURE).unwrap();
    let text = doc.page_text(0).unwrap();
    // 1行目 "See https://..." の 'h'(5文字目、添字4)の箱の左半分と右半分
    let h = &text.chars[4];
    assert_eq!(h.text, "h");
    let (cx, cy) = (
        h.rect.x + h.rect.width / 2.0,
        h.rect.y + h.rect.height / 2.0,
    );
    assert_eq!(text.caret_at(cx - 1.0, cy), Some(4));
    assert_eq!(text.caret_at(cx + 1.0, cy), Some(5));
    // 1行目全体を選ぶと、その行の文字列になる
    let line = text.line_range(4);
    assert_eq!(
        text.string_in(line),
        "See https://example.com/a/doc.pdf for details."
    );
}

/// 回転したページでは、文字の進む向きが下向きになり、キャレットもその向きに従う。
#[test]
fn caret_follows_the_direction_on_a_rotated_page() {
    let doc = PdfDocument::open(FIXTURE).unwrap();
    let text = doc.page_text(1).unwrap();
    let first = &text.chars[0];
    assert!(
        first.dir.0.abs() < 0.01 && first.dir.1 > 0.99,
        "{:?}",
        first.dir
    );
    let (cx, cy) = (
        first.rect.x + first.rect.width / 2.0,
        first.rect.y + first.rect.height / 2.0,
    );
    // 進む向き(下)の手前(上)は0、先(下)は1
    assert_eq!(text.caret_at(cx, cy - 1.0), Some(0));
    assert_eq!(text.caret_at(cx, cy + 1.0), Some(1));
    // 範囲選択の文字列は、回転していても読める順
    assert_eq!(text.string_in(0..7), "Rotated");
}

#[test]
fn text_is_available_without_waiting_only_after_it_was_extracted() {
    let doc = PdfDocument::open(FIXTURE).unwrap();
    assert!(doc.page_text_if_ready(0).is_none());
    doc.page_text(0).unwrap();
    assert!(doc.page_text_if_ready(0).is_some());
    assert!(doc.page_text_if_ready(99).is_none());
}

/// 太字を擬似的に表すため、同じ文字を少しずらして重ね描きしたPDFでも、文字は二重に取れない。
/// 一方、本当に同じ文字が隣り合っているもの("ll", "oo", "kk")は、そのまま残る。
#[test]
fn overdrawn_text_is_not_duplicated() {
    let doc = PdfDocument::open(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/with_double_strike.pdf"
    ))
    .unwrap();
    assert_eq!(
        doc.page_text(0).unwrap().to_plain_string(),
        "Heading text\nHello bookkeeper"
    );
}
