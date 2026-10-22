//! 実際のTesseractを使うOCRのテスト。Tesseractが入っていない環境では、何もせず通る。

use std::process::Command;

use pdf_core::{LinkTarget, PdfDocument};

const IMAGE_ONLY: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/image_only.pdf");

fn tesseract_available() -> bool {
    Command::new("tesseract").arg("--version").output().is_ok()
}

/// 画像だけのPDFは、OCRするまで文字がなく、OCRすると読み取った文字になる。
#[test]
fn ocr_reads_text_from_an_image_only_page() {
    if !tesseract_available() {
        eprintln!("skipped: tesseract is not installed");
        return;
    }
    let doc = PdfDocument::open(IMAGE_ONLY).unwrap();
    assert!(doc.page_text(0).unwrap().chars.is_empty());
    assert!(!doc.is_ocr_done(0));

    let text = doc.ocr_page(0).unwrap().to_plain_string();
    assert!(text.contains("OCR test"), "{text}");
    assert!(text.contains("hello world"), "{text}");
    assert!(text.contains("日本語の文字認識"), "{text}");
    assert!(doc.is_ocr_done(0));

    // OCR後は、通常の文字の取り出しもOCRの結果を返す。
    assert_eq!(doc.page_text(0).unwrap().to_plain_string(), text);
    assert!(
        doc.page_text_if_ready(0)
            .is_some_and(|t| !t.chars.is_empty())
    );
}

/// 認識した文字の位置は、ページ(ポイント単位)の中に収まる。
#[test]
fn ocr_boxes_lie_within_the_page() {
    if !tesseract_available() {
        return;
    }
    let doc = PdfDocument::open(IMAGE_ONLY).unwrap();
    let (w, h) = doc.page_size(0).unwrap();
    let text = doc.ocr_page(0).unwrap();
    assert!(!text.chars.is_empty());
    for c in &text.chars {
        let r = &c.rect;
        assert!(r.x >= 0.0 && r.y >= 0.0, "{c:?}");
        assert!(
            r.x + r.width <= w + 1.0 && r.y + r.height <= h + 1.0,
            "{c:?} in {w}x{h}"
        );
    }
}

const IMAGE_WITH_URL: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/image_with_url.pdf"
);

/// OCRで読み取ったURLが、リンクになる(OCR前は無い)。リンクの位置は、そのURLの文字の上にある。
#[test]
fn ocr_makes_urls_in_images_clickable() {
    if !tesseract_available() {
        return;
    }
    let doc = PdfDocument::open(IMAGE_WITH_URL).unwrap();
    assert!(doc.page_links(0).unwrap().is_empty());

    let text = doc.ocr_page(0).unwrap();
    let links = doc.page_links(0).unwrap();
    let uris: Vec<_> = links
        .iter()
        .filter_map(|l| match &l.target {
            LinkTarget::Uri(u) => Some(u.as_str()),
            LinkTarget::Page(_) => None,
        })
        .collect();
    assert_eq!(uris.len(), 1, "{uris:?} from {:?}", text.to_plain_string());
    assert!(
        uris[0].starts_with("https://example.com/docs/ocr"),
        "{uris:?}"
    );
    // 準備済みのリンクとしても取り出せる。
    assert_eq!(doc.page_links_if_ready(0).unwrap().len(), links.len());
    // 「no link on this line」の行(下半分)にはリンクがない。
    assert!(links[0].rect.y < 70.0, "{:?}", links[0].rect);
}
