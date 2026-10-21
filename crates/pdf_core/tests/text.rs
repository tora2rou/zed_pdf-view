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
