use pdf_core::{PdfDocument, PdfError};

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/two_pages.pdf");

#[test]
fn open_valid_pdf() {
    assert!(PdfDocument::open(FIXTURE).is_ok());
}

#[test]
fn open_missing_file_is_io_error() {
    assert!(matches!(
        PdfDocument::open("/nonexistent.pdf"),
        Err(PdfError::Io(_))
    ));
}

#[test]
fn open_garbage_is_load_error() {
    let path = std::env::temp_dir().join("pdf_core_garbage.pdf");
    std::fs::write(&path, b"not a pdf").unwrap();
    assert!(matches!(PdfDocument::open(&path), Err(PdfError::Load(_))));
}

#[test]
fn page_count_matches() {
    assert_eq!(PdfDocument::open(FIXTURE).unwrap().page_count(), 2);
}

#[test]
fn page_size_per_page() {
    let doc = PdfDocument::open(FIXTURE).unwrap();
    assert_eq!(doc.page_size(0).unwrap(), (595.0, 842.0));
    assert_eq!(doc.page_size(1).unwrap(), (300.0, 200.0));
}

#[test]
fn page_size_out_of_range() {
    let doc = PdfDocument::open(FIXTURE).unwrap();
    assert!(matches!(
        doc.page_size(2),
        Err(PdfError::PageOutOfRange { index: 2, count: 2 })
    ));
}

fn pixel(page: &pdf_core::RenderedPage, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * page.width + x) * 4) as usize;
    page.rgba[i..i + 4].try_into().unwrap()
}

#[test]
fn render_page_dimensions_follow_scale() {
    let doc = PdfDocument::open(FIXTURE).unwrap();
    let page = doc.render_page(1, 2.0).unwrap();
    assert_eq!((page.width, page.height), (600, 400));
    assert_eq!(page.rgba.len(), 600 * 400 * 4);
}

#[test]
fn render_page_draws_content_on_white() {
    let doc = PdfDocument::open(FIXTURE).unwrap();
    let page = doc.render_page(1, 1.0).unwrap();
    // 青い矩形はPDF座標(0,0)-(150,100) = 画面左下
    assert_eq!(pixel(&page, 10, 190), [0, 0, 255, 255]);
    assert_eq!(pixel(&page, 290, 10), [255, 255, 255, 255]);
}

#[test]
fn render_page_rejects_bad_scale() {
    let doc = PdfDocument::open(FIXTURE).unwrap();
    for s in [0.0, -1.0, f32::NAN, 1000.0] {
        assert!(
            matches!(doc.render_page(0, s), Err(PdfError::InvalidScale(_))),
            "{s}"
        );
    }
}

#[test]
fn render_page_out_of_range() {
    let doc = PdfDocument::open(FIXTURE).unwrap();
    assert!(matches!(
        doc.render_page(5, 1.0),
        Err(PdfError::PageOutOfRange { .. })
    ));
}

#[test]
fn render_page_is_cached_per_page_and_scale() {
    use std::sync::Arc;
    let doc = PdfDocument::open(FIXTURE).unwrap();
    let a = doc.render_page(0, 1.0).unwrap();
    assert!(Arc::ptr_eq(&a, &doc.render_page(0, 1.0).unwrap()));
    assert!(!Arc::ptr_eq(&a, &doc.render_page(0, 2.0).unwrap()));
    assert!(!Arc::ptr_eq(&a, &doc.render_page(1, 1.0).unwrap()));
}

#[test]
fn cache_evicts_least_recently_used() {
    use std::{num::NonZeroUsize, sync::Arc};
    let doc = PdfDocument::open(FIXTURE).unwrap();
    doc.set_cache_capacity(NonZeroUsize::new(1).unwrap());
    let a = doc.render_page(0, 1.0).unwrap();
    doc.render_page(1, 1.0).unwrap();
    assert!(!Arc::ptr_eq(&a, &doc.render_page(0, 1.0).unwrap()));
}

#[test]
fn document_is_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<PdfDocument>();
}

#[test]
fn uncached_render_matches_cached_and_leaves_the_cache_alone() {
    use std::{num::NonZeroUsize, sync::Arc};
    let doc = PdfDocument::open(FIXTURE).unwrap();
    doc.set_cache_capacity(NonZeroUsize::new(1).unwrap());
    let cached = doc.render_page(0, 1.0).unwrap();

    let thumb = doc.render_page_uncached(1, 0.5).unwrap();
    assert_eq!((thumb.width, thumb.height), (150, 100));
    assert_eq!(doc.render_page_uncached(0, 1.0).unwrap(), *cached);

    // キャッシュは容量1なので、キャッシュ付きで別ページを描くと追い出される。
    // キャッシュなしの描画では、ページ0は追い出されない。
    assert!(Arc::ptr_eq(&cached, &doc.render_page(0, 1.0).unwrap()));
}

#[test]
fn uncached_render_validates_the_scale() {
    let doc = PdfDocument::open(FIXTURE).unwrap();
    assert!(matches!(
        doc.render_page_uncached(0, 0.0),
        Err(PdfError::InvalidScale(_))
    ));
}
