//! EPUBのパッケージ(ZIPの中身)の読み取り: `container.xml` → OPF → メタデータ・資源・読む順(spine)。

use std::{
    collections::HashMap,
    io::{Read, Seek},
};

use zip::ZipArchive;

/// `container.xml`の場所。
const CONTAINER: &str = "META-INF/container.xml";
/// 1つのファイルとして読む大きさの上限。壊れた/悪意あるEPUBへの安全弁(ZIP爆弾)。
const MAX_ENTRY_BYTES: u64 = 256 * 1024 * 1024;

/// 本に含まれる資源(章のXHTML、画像など)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub id: String,
    /// ZIPの中のパス(OPFからの相対を、ZIPのルートからのパスにしたもの)。
    pub path: String,
    pub media_type: String,
}

/// 読み取った本の構造。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Package {
    pub title: Option<String>,
    pub creators: Vec<String>,
    pub language: Option<String>,
    /// 読む順の章(資源の番号ではなく、実体)。
    pub spine: Vec<Item>,
    /// すべての資源(`path`をキーに引ける)。
    pub items: Vec<Item>,
}

impl Package {
    #[cfg(test)]
    pub fn item_by_path(&self, path: &str) -> Option<&Item> {
        self.items.iter().find(|i| i.path == path)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct PackageError(pub String);

impl std::fmt::Display for PackageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

fn err(message: impl Into<String>) -> PackageError {
    PackageError(message.into())
}

/// ZIPから、1つのファイルを読む。
pub fn read_entry<R: Read + Seek>(
    zip: &mut ZipArchive<R>,
    path: &str,
) -> Result<Vec<u8>, PackageError> {
    let file = zip
        .by_name(path)
        .map_err(|e| err(format!("{path}を読めません: {e}")))?;
    let mut bytes = Vec::new();
    file.take(MAX_ENTRY_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| err(format!("{path}を読めません: {e}")))?;
    if bytes.len() as u64 > MAX_ENTRY_BYTES {
        return Err(err(format!("{path}が大きすぎます")));
    }
    Ok(bytes)
}

/// EPUB(ZIP)を開いて、構造を読む。
pub fn read<R: Read + Seek>(zip: &mut ZipArchive<R>) -> Result<Package, PackageError> {
    let container = String::from_utf8_lossy(&read_entry(zip, CONTAINER)?).into_owned();
    let opf_path = opf_path(&container)?;
    let opf = String::from_utf8_lossy(&read_entry(zip, &opf_path)?).into_owned();
    parse_opf(&opf, &opf_path)
}

/// `container.xml`から、OPFのパスを取る。
fn opf_path(container: &str) -> Result<String, PackageError> {
    let doc = roxmltree::Document::parse(container)
        .map_err(|e| err(format!("container.xmlを解釈できません: {e}")))?;
    doc.descendants()
        .find(|n| n.has_tag_name("rootfile"))
        .and_then(|n| n.attribute("full-path"))
        .map(str::to_string)
        .ok_or_else(|| err("container.xmlにOPFの場所がありません"))
}

/// OPF(`opf_path`にある)を読む。資源のパスは、ZIPのルートからのパスにする。
pub fn parse_opf(opf: &str, opf_path: &str) -> Result<Package, PackageError> {
    let doc =
        roxmltree::Document::parse(opf).map_err(|e| err(format!("OPFを解釈できません: {e}")))?;
    let base = opf_path.rsplit_once('/').map_or("", |(dir, _)| dir);
    let text_of = |name: &str| {
        doc.descendants()
            .find(|n| n.has_tag_name(name))
            .and_then(|n| n.text())
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
    };

    let items: Vec<Item> = doc
        .descendants()
        .filter(|n| {
            n.has_tag_name("item") && n.parent().is_some_and(|p| p.has_tag_name("manifest"))
        })
        .filter_map(|n| {
            Some(Item {
                id: n.attribute("id")?.to_string(),
                path: resolve(base, &percent_decode(n.attribute("href")?)),
                media_type: n.attribute("media-type").unwrap_or_default().to_string(),
            })
        })
        .collect();
    let by_id: HashMap<&str, &Item> = items.iter().map(|i| (i.id.as_str(), i)).collect();
    let spine: Vec<Item> = doc
        .descendants()
        .filter(|n| n.has_tag_name("itemref"))
        .filter(|n| n.attribute("linear") != Some("no"))
        .filter_map(|n| by_id.get(n.attribute("idref")?).map(|i| (*i).clone()))
        .collect();
    if spine.is_empty() {
        return Err(err("読む章(spine)がありません"));
    }
    Ok(Package {
        title: text_of("title"),
        creators: doc
            .descendants()
            .filter(|n| n.has_tag_name("creator"))
            .filter_map(|n| n.text())
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect(),
        language: text_of("language"),
        spine,
        items,
    })
}

/// `base`(ZIPの中のフォルダ)から見た相対パス`href`を、ZIPのルートからのパスにする。`..`と`.`を畳む。
pub fn resolve(base: &str, href: &str) -> String {
    let href = href.split(['#', '?']).next().unwrap_or(href);
    let mut parts: Vec<&str> = base.split('/').filter(|p| !p.is_empty()).collect();
    for part in href.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    parts.join("/")
}

/// `%E3%81%82`のようなパーセントエンコードを戻す。
pub fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Some(byte) = std::str::from_utf8(&bytes[i + 1..i + 3])
                .ok()
                .and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            out.push(byte);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    const OPF: &str = r#"<?xml version="1.0"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:title> 吾輩は猫である </dc:title>
    <dc:creator>夏目漱石</dc:creator>
    <dc:language>ja</dc:language>
  </metadata>
  <manifest>
    <item id="c1" href="text/ch%201.xhtml" media-type="application/xhtml+xml"/>
    <item id="c2" href="text/ch2.xhtml" media-type="application/xhtml+xml"/>
    <item id="img" href="../images/a.png" media-type="image/png"/>
  </manifest>
  <spine>
    <itemref idref="c1"/>
    <itemref idref="img" linear="no"/>
    <itemref idref="c2"/>
    <itemref idref="missing"/>
  </spine>
</package>"#;

    #[test]
    fn metadata_is_read() {
        let p = parse_opf(OPF, "OEBPS/content.opf").unwrap();
        assert_eq!(p.title.as_deref(), Some("吾輩は猫である"));
        assert_eq!(p.creators, ["夏目漱石"]);
        assert_eq!(p.language.as_deref(), Some("ja"));
    }

    /// 読む順は、spineの順。`linear="no"`と、存在しない参照は飛ばす。パスはZIPのルートからになる。
    #[test]
    fn the_spine_is_the_reading_order() {
        let p = parse_opf(OPF, "OEBPS/content.opf").unwrap();
        let paths: Vec<_> = p.spine.iter().map(|i| i.path.as_str()).collect();
        assert_eq!(paths, ["OEBPS/text/ch 1.xhtml", "OEBPS/text/ch2.xhtml"]);
    }

    #[test]
    fn resources_resolve_relative_to_the_opf() {
        let p = parse_opf(OPF, "OEBPS/content.opf").unwrap();
        assert!(p.item_by_path("images/a.png").is_some(), "{:?}", p.items);
        assert!(p.item_by_path("OEBPS/text/ch2.xhtml").is_some());
    }

    #[test]
    fn an_opf_at_the_root_resolves_from_the_root() {
        let p = parse_opf(OPF, "content.opf").unwrap();
        assert_eq!(p.spine[0].path, "text/ch 1.xhtml");
    }

    #[test]
    fn a_book_without_a_spine_is_an_error() {
        let opf = r#"<package><manifest><item id="a" href="a.xhtml" media-type="x"/></manifest><spine/></package>"#;
        assert!(parse_opf(opf, "c.opf").is_err());
    }

    #[test]
    fn broken_xml_is_an_error() {
        assert!(parse_opf("<package>", "c.opf").is_err());
    }

    #[test]
    fn container_points_to_the_opf() {
        let c = r#"<container><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="x"/></rootfiles></container>"#;
        assert_eq!(opf_path(c).unwrap(), "OEBPS/content.opf");
        assert!(opf_path("<container/>").is_err());
    }

    #[test]
    fn paths_are_normalised() {
        assert_eq!(resolve("a/b", "../c.png"), "a/c.png");
        assert_eq!(resolve("a", "./x/./y.html#frag"), "a/x/y.html");
        assert_eq!(resolve("", "x.html"), "x.html");
        assert_eq!(resolve("a", "../../up.html"), "up.html");
    }

    #[test]
    fn percent_encoding_is_decoded() {
        assert_eq!(percent_decode("a%20b%E3%81%82"), "a bあ");
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("%zz"), "%zz");
    }
}
