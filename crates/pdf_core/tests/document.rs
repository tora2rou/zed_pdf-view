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
