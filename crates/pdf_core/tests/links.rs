use pdf_core::{Link, LinkRect, LinkTarget, PdfDocument, PdfError};

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/with_links.pdf");
const NO_LINKS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/two_pages.pdf");

fn rect(x: f32, y: f32, width: f32, height: f32) -> LinkRect {
    LinkRect {
        x,
        y,
        width,
        height,
    }
}

fn link(rect: LinkRect, target: LinkTarget) -> Link {
    Link { rect, target }
}

fn uri(s: &str) -> LinkTarget {
    LinkTarget::Uri(s.to_string())
}

#[test]
fn page_without_annotations_has_no_links() {
    let doc = PdfDocument::open(NO_LINKS).unwrap();
    assert!(doc.page_links(0).unwrap().is_empty());
}

#[test]
fn out_of_range_page_is_an_error() {
    let doc = PdfDocument::open(FIXTURE).unwrap();
    assert!(matches!(
        doc.page_links(3),
        Err(PdfError::PageOutOfRange { index: 3, count: 3 })
    ));
}

#[test]
fn reads_all_link_kinds_in_document_order() {
    let doc = PdfDocument::open(FIXTURE).unwrap();
    assert_eq!(
        doc.page_links(0).unwrap(),
        [
            // /Dest の配列 → ページ2
            link(rect(10.0, 70.0, 50.0, 20.0), LinkTarget::Page(1)),
            // URIアクション
            link(rect(100.0, 20.0, 80.0, 30.0), uri("https://example.com/a")),
            // 危険なスキームも、読み取ったまま返す(開くかどうかは呼び出し側が決める)
            link(rect(100.0, 70.0, 40.0, 20.0), uri("javascript:alert(1)")),
            // GoToアクション → ページ3
            link(rect(70.0, 40.0, 20.0, 20.0), LinkTarget::Page(2)),
            // 矩形の向きが逆でも、正規化される
            link(rect(150.0, 5.0, 40.0, 10.0), uri("mailto:me@example.com")),
        ]
    );
}

#[test]
fn rotated_and_cropped_pages_use_display_coordinates() {
    let doc = PdfDocument::open(FIXTURE).unwrap();
    // ページ2は90度回転、ページ3はクロップボックスで切り出されている。
    // どちらも、表示される画像の座標で返る(次のテストで描画と突き合わせる)。
    assert_eq!(doc.page_links(1).unwrap().len(), 1);
    assert_eq!(doc.page_links(1).unwrap()[0].target, LinkTarget::Page(0));
    assert_eq!(
        doc.page_links(2).unwrap(),
        [link(
            // クロップ後の表示は160x80。ユーザー空間のy 20..40は、下端(10)を引いて10..30、
            // 上下を反転して 80 - 30 = 50 から高さ20。xは 30 - 20 = 10 から幅40。
            rect(10.0, 50.0, 40.0, 20.0),
            uri("http://example.org/crop")
        )]
    );
}

/// リンクの矩形の中心は、そこに塗られた色の上にある。
/// 回転やクロップボックスがあっても、描画された画像と位置が合っていることを確かめる。
#[test]
fn link_rects_line_up_with_the_rendered_page() {
    let doc = PdfDocument::open(FIXTURE).unwrap();
    // (ページ, リンクの番号, 塗られている色)
    let cases: [(usize, usize, [u8; 3]); 7] = [
        (0, 0, [255, 0, 0]),
        (0, 1, [0, 255, 0]),
        (0, 2, [0, 0, 255]),
        (0, 3, [255, 255, 0]),
        (0, 4, [128, 128, 128]),
        (1, 0, [255, 0, 255]), // 回転ページ
        (2, 0, [0, 255, 255]), // クロップボックス付きページ
    ];
    for (page, index, expected) in cases {
        let image = doc.render_page(page, 1.0).unwrap();
        let r = doc.page_links(page).unwrap()[index].rect;
        let (cx, cy) = ((r.x + r.width / 2.0) as u32, (r.y + r.height / 2.0) as u32);
        let i = ((cy * image.width + cx) * 4) as usize;
        let pixel = &image.rgba[i..i + 3];
        // 色は多少ずれてもよい(アンチエイリアスの影響を避けて矩形の中心を見ている)
        for (got, want) in pixel.iter().zip(expected) {
            assert!(
                got.abs_diff(want) <= 2,
                "page {page} link {index}: pixel at ({cx}, {cy}) is {pixel:?}, expected {expected:?}"
            );
        }
    }
}

#[test]
fn links_are_cached_per_page() {
    let doc = PdfDocument::open(FIXTURE).unwrap();
    let first = doc.page_links(0).unwrap().as_ptr();
    assert_eq!(first, doc.page_links(0).unwrap().as_ptr());
}
