use pdf_core::{PdfDocument, PdfError};

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/two_pages.pdf");

#[test]
fn open_valid_pdf() {
    assert!(PdfDocument::open(FIXTURE).is_ok());
}

#[test]
fn open_missing_file_is_io_error() {
    assert!(matches!(PdfDocument::open("/nonexistent.pdf"), Err(PdfError::Io(_))));
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
