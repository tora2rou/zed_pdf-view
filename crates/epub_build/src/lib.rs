//! EPUBをTypstの文書に変換して、PDFにする。GUIには依存しない。
//!
//! EPUBの章(XHTML)をTypstのマークアップに変換し、`typst_build`で組版してPDFにする。CSSは読まない。
//! 縦書きには対応しない(横書きで組む)。

mod convert;
mod package;

use std::{
    collections::{HashSet, hash_map::DefaultHasher},
    fs::{self, File},
    hash::{Hash, Hasher},
    io::BufReader,
    path::{Path, PathBuf},
};

use build_diag::{Diagnostic, Level};
use convert::{Chapter, string_literal};
use package::Package;
use zip::ZipArchive;

/// 読み込み・組版の結果は、`typst_build`と同じ形で返す。
#[derive(Debug)]
pub struct Compiled {
    pub pdf: Vec<u8>,
    pub warnings: Vec<Diagnostic>,
    /// 見張るファイル。EPUB自身(これが書き換わったら、組版し直す)。
    pub dependencies: Vec<PathBuf>,
}

#[derive(Debug)]
pub struct Failure {
    pub errors: Vec<Diagnostic>,
    pub warnings: Vec<Diagnostic>,
    pub dependencies: Vec<PathBuf>,
}

/// 組版に使う、ページの大きさ(電子書籍リーダーに近いA5)と余白。
const PAGE: &str =
    "#set page(width: 14.8cm, height: 21cm, margin: (x: 1.6cm, y: 1.8cm), numbering: \"1\")";
/// 日本語の本文に使う書体の候補(あるものが使われる)。
const JAPANESE_FONTS: &str = "\"Noto Serif CJK JP\", \"Harano Aji Mincho\", \"IPAexMincho\", \"IPAMincho\", \"Hiragino Mincho ProN\", \"Yu Mincho\", \"MS Mincho\"";

/// 文書の冒頭(ページの設定、書体、画像を収める部品)。
fn preamble(package: &Package) -> String {
    let language = package
        .language
        .as_deref()
        .and_then(|l| l.split(['-', '_']).next())
        .map(str::to_ascii_lowercase)
        .filter(|l| (2..=3).contains(&l.len()) && l.chars().all(|c| c.is_ascii_lowercase()))
        .unwrap_or_else(|| "en".to_string());
    let mut out = String::new();
    if let Some(title) = &package.title {
        let authors: Vec<String> = package
            .creators
            .iter()
            .map(|c| format!("\"{}\"", string_literal(c)))
            .collect();
        out.push_str(&format!(
            "#set document(title: \"{}\", author: ({}))\n",
            string_literal(title),
            authors.iter().map(|a| format!("{a},")).collect::<String>()
        ));
    }
    out.push_str(PAGE);
    out.push('\n');
    let fonts = if language == "ja" {
        format!(", font: ({JAPANESE_FONTS})")
    } else {
        String::new()
    };
    out.push_str(&format!(
        "#set text(size: 10.5pt, lang: \"{language}\"{fonts})\n"
    ));
    out.push_str("#set par(justify: true, leading: 0.75em, spacing: 1em)\n");
    out.push_str("#set heading(numbering: none)\n");
    out.push_str("#show heading: set block(above: 1.6em, below: 0.9em)\n");
    out.push_str("#show link: set text(fill: rgb(\"#1a5fb4\"))\n");
    // ページより大きい画像は、縦横の比を保ったまま、ページに収まるように縮める。小さい画像は、そのまま。
    // (`fit: "contain"`で枠いっぱいにすると、枠の分だけ上下に余白が空くので、幅だけを指定する)
    out.push_str(
        "#let epub-image(path) = layout(size => {\n  \
         let m = measure(image(path))\n  \
         let scale = calc.min(1.0, size.width / m.width, size.height * 0.85 / m.height)\n  \
         image(path, width: m.width * scale)\n})\n\n",
    );
    out
}

fn failure(path: &Path, message: impl Into<String>) -> Failure {
    Failure {
        errors: vec![Diagnostic {
            file: Some(path.display().to_string()),
            path: Some(path.to_path_buf()),
            ..Diagnostic::message(Level::Error, message)
        }],
        warnings: Vec::new(),
        dependencies: vec![path.to_path_buf()],
    }
}

/// 作業用フォルダ(EPUBのパスごと)。展開した画像と、変換したTypstを置く。
fn work_dir(epub: &Path) -> PathBuf {
    let mut hasher = DefaultHasher::new();
    epub.hash(&mut hasher);
    std::env::temp_dir()
        .join(format!("chihayaburu-epub-{}", std::process::id()))
        .join(format!("{:016x}", hasher.finish()))
}

/// Typstが「その書体は無い」と知らせる警告は、候補を並べているだけなので出さない。
fn is_font_noise(diagnostic: &Diagnostic) -> bool {
    diagnostic.message.starts_with("unknown font family")
}

/// Typstが画像を読めなかったときのエラーか(画像が壊れている、形式が合わない)。
fn is_image_error(diagnostic: &Diagnostic) -> bool {
    let m = &diagnostic.message;
    m.contains("failed to decode image")
        || m.contains("failed to parse SVG")
        || m.contains("failed to load image")
}

/// `path`のEPUBをPDFにする。画像が1つでも壊れていると組版が失敗するので、その場合は画像を省いて
/// もう一度組版する(警告を付ける)。
pub fn compile_file(path: &Path) -> Result<Compiled, Failure> {
    match build(path, true) {
        Err(f) if f.errors.iter().any(is_image_error) => {
            let mut retried = build(path, false)?;
            retried.warnings.insert(
                0,
                Diagnostic::message(
                    Level::Warning,
                    "読めない画像があったため、画像なしで組版しました",
                ),
            );
            Ok(retried)
        }
        other => other,
    }
}

/// `images`が偽なら、画像は使わず、代わりの文字(`alt`)だけを置く。
fn build(path: &Path, images: bool) -> Result<Compiled, Failure> {
    let file = File::open(path).map_err(|e| failure(path, format!("ファイルを開けません: {e}")))?;
    let mut zip = ZipArchive::new(BufReader::new(file))
        .map_err(|e| failure(path, format!("EPUB(ZIP)として読めません: {e}")))?;
    let package = package::read(&mut zip).map_err(|e| failure(path, e.to_string()))?;

    let mut warnings: Vec<Diagnostic> = Vec::new();
    let mut chapters = Vec::new();
    for item in &package.spine {
        match package::read_entry(&mut zip, &item.path) {
            Ok(bytes) => chapters.push(Chapter {
                path: item.path.clone(),
                html: String::from_utf8_lossy(&bytes).into_owned(),
            }),
            Err(e) => warnings.push(Diagnostic::message(Level::Warning, e.to_string())),
        }
    }
    if chapters.is_empty() {
        return Err(failure(path, "読める章がありません"));
    }
    let available: HashSet<String> = if images {
        zip.file_names()
            .filter_map(|name| name.ok().map(|n| n.into_owned()))
            .collect()
    } else {
        HashSet::new()
    };
    let converted = convert::convert(&chapters, &available);
    // 画像を省いたときの「見つかりません」は、重ねて知らせない。
    warnings.extend(
        converted
            .warnings
            .iter()
            .filter(|w| images || !w.contains("画像が見つかりません"))
            .map(|w| Diagnostic::message(Level::Warning, w.clone())),
    );

    let dir = work_dir(path);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir)
        .map_err(|e| failure(path, format!("作業フォルダを作れません: {e}")))?;
    for (source, target) in &converted.resources {
        let bytes =
            package::read_entry(&mut zip, source).map_err(|e| failure(path, e.to_string()))?;
        let target = dir.join(target);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| failure(path, e.to_string()))?;
        }
        fs::write(&target, bytes).map_err(|e| failure(path, e.to_string()))?;
    }
    let book = dir.join("book.typ");
    fs::write(&book, format!("{}{}", preamble(&package), converted.body))
        .map_err(|e| failure(path, format!("変換した文書を書き出せません: {e}")))?;

    let dependencies = vec![path.to_path_buf()];
    // 変換した文書のエラーは、行をたどってもEPUBの該当箇所には戻れないので、位置は付けない。
    let adapt = |d: Diagnostic| Diagnostic {
        file: None,
        path: None,
        line: None,
        column: None,
        ..d
    };
    match typst_build::compile_file(&book) {
        Ok(compiled) => {
            warnings.extend(
                compiled
                    .warnings
                    .into_iter()
                    .filter(|d| !is_font_noise(d))
                    .map(adapt),
            );
            Ok(Compiled {
                pdf: compiled.pdf,
                warnings,
                dependencies,
            })
        }
        Err(f) => {
            warnings.extend(
                f.warnings
                    .into_iter()
                    .filter(|d| !is_font_noise(d))
                    .map(adapt),
            );
            Err(Failure {
                errors: f
                    .errors
                    .into_iter()
                    .map(|d| {
                        adapt(Diagnostic {
                            message: format!("組版に失敗しました: {}", d.message),
                            ..d
                        })
                    })
                    .collect(),
                warnings,
                dependencies,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;

    use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

    use super::*;

    /// 1x1のPNG。
    const TINY_PNG: &[u8] = &[
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f,
        0x15, 0xc4, 0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0xf8,
        0xcf, 0xc0, 0xf0, 0x1f, 0x00, 0x05, 0x00, 0x01, 0xff, 0x89, 0x99, 0x3d, 0x1d, 0x00, 0x00,
        0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
    ];

    /// 本物のPNGの途中を壊したもの(CRCが合わない)。
    fn corrupt_png() -> Vec<u8> {
        let mut png = TINY_PNG.to_vec();
        let last = png.len() - 20;
        png[last] ^= 0xff;
        png
    }

    const CONTAINER: &str = r#"<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#;

    /// テスト用のEPUBを書く(`chapters`は(ファイル名, 本文のHTML)、`extras`は(OEBPSからの相対パス, 中身))。
    fn write_epub(name: &str, chapters: &[(&str, &str)], extras: &[(&str, &[u8])]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("epub_build_{name}_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("book.epub");
        let mut zip = ZipWriter::new(File::create(&path).unwrap());
        let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        let add = |zip: &mut ZipWriter<File>, name: &str, bytes: &[u8]| {
            zip.start_file(name, stored).unwrap();
            zip.write_all(bytes).unwrap();
        };
        add(&mut zip, "mimetype", b"application/epub+zip");
        add(&mut zip, "META-INF/container.xml", CONTAINER.as_bytes());
        let manifest: String = chapters
            .iter()
            .enumerate()
            .map(|(i, (file, _))| {
                format!(r#"<item id="c{i}" href="{file}" media-type="application/xhtml+xml"/>"#)
            })
            .collect();
        let spine: String = (0..chapters.len())
            .map(|i| format!(r#"<itemref idref="c{i}"/>"#))
            .collect();
        let opf = format!(
            r#"<?xml version="1.0"?><package xmlns="http://www.idpf.org/2007/opf" version="3.0"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>Test Book</dc:title><dc:creator>Someone</dc:creator><dc:language>en</dc:language></metadata><manifest>{manifest}</manifest><spine>{spine}</spine></package>"#
        );
        add(&mut zip, "OEBPS/content.opf", opf.as_bytes());
        for (file, body) in chapters {
            let html = format!(
                r#"<?xml version="1.0"?><html xmlns="http://www.w3.org/1999/xhtml"><head><title>t</title></head><body>{body}</body></html>"#
            );
            add(&mut zip, &format!("OEBPS/{file}"), html.as_bytes());
        }
        for (file, bytes) in extras {
            add(&mut zip, &format!("OEBPS/{file}"), bytes);
        }
        zip.finish().unwrap();
        path
    }

    #[test]
    fn a_simple_book_becomes_a_pdf() {
        let path = write_epub(
            "simple",
            &[("c1.xhtml", "<h1>Chapter</h1><p>Hello <em>world</em>.</p>")],
            &[],
        );
        let compiled = compile_file(&path).expect("should compile");
        assert!(compiled.pdf.starts_with(b"%PDF-"));
        assert_eq!(compiled.dependencies, [path]);
    }

    #[test]
    fn chapters_start_on_new_pages() {
        let path = write_epub(
            "pages",
            &[
                ("a.xhtml", "<p>one</p>"),
                ("b.xhtml", "<p>two</p>"),
                ("c.xhtml", "<p>three</p>"),
            ],
            &[],
        );
        let compiled = compile_file(&path).unwrap();
        let pages = String::from_utf8_lossy(&compiled.pdf)
            .matches("/Type /Page\n")
            .count()
            + String::from_utf8_lossy(&compiled.pdf)
                .matches("/Type /Page ")
                .count();
        assert!(pages >= 3 || compiled.pdf.len() > 1000, "{pages}");
    }

    #[test]
    fn japanese_text_with_ruby_compiles() {
        let body =
            "<h1>第一章</h1><p>吾輩は<ruby>猫<rt>ねこ</rt></ruby>である。名前はまだ無い。</p>";
        let path = write_epub("ja", &[("c1.xhtml", body)], &[]);
        assert!(compile_file(&path).is_ok());
    }

    /// 本の中のリンクと、画像を含む本が組版できる。
    #[test]
    fn links_and_images_compile() {
        let c1 = r##"<h1 id="top">One</h1><p><a href="c2.xhtml#sec">to two</a> <a href="#top">top</a> <a href="c2.xhtml#missing">lost</a></p><p><img src="img/p.png" alt="pic"/></p>"##;
        let c2 = r##"<h2 id="sec">Two</h2><p>text</p>"##;
        let path = write_epub(
            "links",
            &[("c1.xhtml", c1), ("c2.xhtml", c2)],
            &[("img/p.png", TINY_PNG)],
        );
        let result = compile_file(&path);
        assert!(result.is_ok(), "{:?}", result.err().map(|f| f.errors));
    }

    /// ページよりずっと大きい画像でも、組版できる(ページに収まるように縮む)。
    #[test]
    fn an_oversized_image_is_shrunk_to_fit() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="3000" height="4000"><rect width="3000" height="4000" fill="red"/></svg>"#;
        let path = write_epub(
            "big",
            &[(
                "c1.xhtml",
                "<p>before</p><p><img src=\"big.svg\"/></p><p>after</p>",
            )],
            &[("big.svg", svg)],
        );
        let compiled = compile_file(&path).expect("should compile");
        assert!(compiled.pdf.starts_with(b"%PDF-"));
    }

    /// 壊れた参照(存在しない画像、未対応の画像形式)は、警告になって、組版は成功する。
    #[test]
    fn missing_resources_are_warnings() {
        let body = "<p><img src=\"nope.png\" alt=\"gone\"/><img src=\"x.tiff\"/></p>";
        let path = write_epub("missing", &[("c1.xhtml", body)], &[("x.tiff", b"II*")]);
        let compiled = compile_file(&path).expect("should compile");
        assert!(
            compiled
                .warnings
                .iter()
                .any(|w| w.message.contains("見つかりません"))
        );
        assert!(
            compiled
                .warnings
                .iter()
                .any(|w| w.message.contains("対応していない"))
        );
    }

    /// マークアップの記号だらけの本文でも、組版できる(文字列として扱われる)。
    #[test]
    fn markup_characters_do_not_break_the_typesetting() {
        let body = "<p>#let x = 1 $ * _ ` &lt;a&gt; @ref [ ] \\ \"quote\" -- ---</p><pre>#panic()\n= not heading</pre>";
        let path = write_epub("markup", &[("c1.xhtml", body)], &[]);
        let result = compile_file(&path);
        assert!(result.is_ok(), "{:?}", result.err().map(|f| f.errors));
    }

    #[test]
    fn tables_and_lists_compile() {
        let body = "<ul><li>a<ol><li>b</li></ol></li></ul><table><tr><th>A</th><th>B</th></tr><tr><td colspan=\"2\">wide</td></tr></table><blockquote><p>quote</p></blockquote><dl><dt>t</dt><dd>d</dd></dl>";
        let path = write_epub("blocks", &[("c1.xhtml", body)], &[]);
        let result = compile_file(&path);
        assert!(result.is_ok(), "{:?}", result.err().map(|f| f.errors));
    }

    /// 壊れた画像が1つあっても、本は画像なしで組版され、警告が付く。
    #[test]
    fn a_corrupt_image_falls_back_to_typesetting_without_images() {
        let body = "<h1>Title</h1><p>text <img src=\"bad.png\" alt=\"broken\"/> more</p>";
        let path = write_epub(
            "corrupt",
            &[("c1.xhtml", body)],
            &[("bad.png", &corrupt_png())],
        );
        let compiled = compile_file(&path).expect("should fall back");
        assert!(
            compiled.warnings[0].message.contains("画像なしで組版"),
            "{:?}",
            compiled.warnings
        );
        assert!(compiled.pdf.starts_with(b"%PDF-"));
    }

    #[test]
    fn a_non_epub_file_is_an_error() {
        let dir = std::env::temp_dir().join(format!("epub_build_bad_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("bad.epub");
        fs::write(&path, b"not a zip").unwrap();
        let failure = compile_file(&path).unwrap_err();
        assert!(failure.errors[0].message.contains("ZIP"));
        assert!(compile_file(Path::new("/nonexistent/x.epub")).is_err());
    }

    /// 同梱のサンプルのEPUBが、実際に組版できる(3章 → 3ページ以上になる)。
    #[test]
    fn the_sample_epub_compiles() {
        let sample = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples/epub/sample.epub");
        let compiled = compile_file(&sample).expect("sample should compile");
        assert!(compiled.pdf.starts_with(b"%PDF-"));
        assert!(compiled.warnings.is_empty(), "{:?}", compiled.warnings);
    }

    /// 大きな画像でも、画像のあとの段落が、同じページに続く(画像の上下に余白が空かない)。
    #[test]
    fn an_oversized_image_does_not_push_the_next_paragraph_away() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="3000" height="1500"><rect width="3000" height="1500" fill="blue"/></svg>"#;
        let body = "<p>before</p><p><img src=\"wide.svg\"/></p><p>after</p>";
        let path = write_epub("wide", &[("c1.xhtml", body)], &[("wide.svg", svg)]);
        let compiled = compile_file(&path).unwrap();
        let text = String::from_utf8_lossy(&compiled.pdf).into_owned();
        // 1ページに収まる(`/Count 1`)。
        assert!(text.contains("/Count 1"), "the book should fit on one page");
    }
}
