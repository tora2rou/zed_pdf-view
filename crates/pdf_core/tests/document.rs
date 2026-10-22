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
fn document_is_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<PdfDocument>();
}

#[test]
fn render_page_renders_the_same_image_every_time() {
    let doc = PdfDocument::open(FIXTURE).unwrap();
    let thumb = doc.render_page(1, 0.5).unwrap();
    assert_eq!((thumb.width, thumb.height), (150, 100));
    assert_eq!(
        doc.render_page(0, 1.0).unwrap(),
        doc.render_page(0, 1.0).unwrap()
    );
}

#[test]
fn render_page_validates_the_scale() {
    let doc = PdfDocument::open(FIXTURE).unwrap();
    assert!(matches!(
        doc.render_page(0, 0.0),
        Err(PdfError::InvalidScale(_))
    ));
}

const OUTLINE_FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/with_outline.pdf"
);

fn item(
    title: &str,
    page: Option<usize>,
    children: Vec<pdf_core::OutlineItem>,
) -> pdf_core::OutlineItem {
    pdf_core::OutlineItem {
        title: title.to_string(),
        page,
        children,
    }
}

#[test]
fn outline_is_empty_without_one() {
    assert!(PdfDocument::open(FIXTURE).unwrap().outline().is_empty());
}

#[test]
fn outline_reads_titles_hierarchy_and_destinations() {
    let doc = PdfDocument::open(OUTLINE_FIXTURE).unwrap();
    assert_eq!(
        doc.outline(),
        [
            // /Dest の配列
            item(
                "Chapter 1",
                Some(0),
                vec![item("Section 1.1", Some(1), vec![])]
            ),
            // /A の GoTo アクション
            item("Chapter 2", Some(2), vec![]),
            // UTF-16BEの題名 + カタログの /Dests にある名前付き宛先
            item("第一章", Some(1), vec![]),
            // /Names の /Dests ツリーにある名前付き宛先
            item("Named in tree", Some(2), vec![]),
        ]
    );
}

/// 目次の`/Next`が自分自身を指している壊れたPDFでも、読み込みが終わる。
#[test]
fn cyclic_outline_terminates() {
    let objs: [&str; 5] = [
        "<< /Type /Catalog /Pages 2 0 R /Outlines 4 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 10 10] >>",
        "<< /Type /Outlines /First 5 0 R /Last 5 0 R >>",
        "<< /Title (loop) /Parent 4 0 R /Next 5 0 R /First 5 0 R /Dest [3 0 R /Fit] >>",
    ];
    let mut pdf = String::from("%PDF-1.4\n");
    let mut offsets = Vec::new();
    for (i, obj) in objs.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.push_str(&format!("{} 0 obj\n{}\nendobj\n", i + 1, obj));
    }
    let xref = pdf.len();
    pdf.push_str(&format!(
        "xref\n0 {}\n0000000000 65535 f \n",
        objs.len() + 1
    ));
    for offset in offsets {
        pdf.push_str(&format!("{offset:010} 00000 n \n"));
    }
    pdf.push_str(&format!(
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
        objs.len() + 1
    ));
    let path = std::env::temp_dir().join(format!("pdf_core_cyclic_{}.pdf", std::process::id()));
    std::fs::write(&path, pdf).unwrap();

    let doc = PdfDocument::open(&path).unwrap();
    let outline = doc.outline();
    assert_eq!(outline.len(), 1);
    assert_eq!(outline[0].title, "loop");
    assert!(outline[0].children.is_empty(), "the cycle must be cut");
    std::fs::remove_file(path).ok();
}
