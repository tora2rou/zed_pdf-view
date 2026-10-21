//! PDFの目次(アウトライン、しおり)の読み取り。

use std::collections::{HashMap, HashSet};

use hayro::hayro_syntax::{
    Pdf,
    object::{Dict, Name, Object, ObjectIdentifier, String as PdfString},
};

/// 読み込む項目数の上限。壊れた/悪意あるPDFで延々と読み続けないための安全弁。
const MAX_ITEMS: usize = 50_000;
/// 階層の深さの上限。
const MAX_DEPTH: usize = 32;
/// 名前付き宛先のツリーをたどる深さの上限。
const MAX_TREE_DEPTH: usize = 16;

/// 目次の1項目。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutlineItem {
    pub title: String,
    /// 移動先のページ(0始まり)。リンク先が読めない・ページ外のときは`None`。
    pub page: Option<usize>,
    pub children: Vec<OutlineItem>,
}

/// 文書全体の目次を読む。目次が無い・読めない場合は空。
pub(crate) fn read_outline(pdf: &Pdf) -> Vec<OutlineItem> {
    let xref = pdf.xref();
    let Some(catalog) = xref.get::<Dict>(xref.root_id()) else {
        return Vec::new();
    };
    let Some(outlines) = catalog.get::<Dict>(b"Outlines") else {
        return Vec::new();
    };
    let pages = pdf
        .pages()
        .iter()
        .enumerate()
        .filter_map(|(index, page)| Some((page.raw().obj_id()?, index)))
        .collect();
    let reader = Reader {
        catalog,
        pages,
        budget: MAX_ITEMS,
        visited: HashSet::new(),
    };
    let mut reader = reader;
    reader.read_siblings(outlines.get::<Dict>(b"First"), 0)
}

struct Reader<'a> {
    catalog: Dict<'a>,
    /// ページオブジェクトの識別子 → ページ番号。
    pages: HashMap<ObjectIdentifier, usize>,
    budget: usize,
    /// 読み込み済みの項目。循環を検出する。
    visited: HashSet<ObjectIdentifier>,
}

impl<'a> Reader<'a> {
    /// `first`から`Next`をたどって、同じ階層の項目を読む。
    fn read_siblings(&mut self, first: Option<Dict<'a>>, depth: usize) -> Vec<OutlineItem> {
        let mut items = Vec::new();
        let mut current = first;
        while let Some(dict) = current {
            if depth >= MAX_DEPTH || self.budget == 0 {
                break;
            }
            if let Some(id) = dict.obj_id()
                && !self.visited.insert(id)
            {
                break; // 循環している
            }
            self.budget -= 1;
            let children = self.read_siblings(dict.get::<Dict>(b"First"), depth + 1);
            items.push(OutlineItem {
                title: dict
                    .get::<PdfString>(b"Title")
                    .map(|s| decode_text_string(s.as_bytes()))
                    .unwrap_or_default(),
                page: self.destination_page(&dict),
                children,
            });
            current = dict.get::<Dict>(b"Next");
        }
        items
    }

    /// 項目の移動先ページ。`/Dest`、または`/A`(GoToアクション)の`/D`から求める。
    fn destination_page(&self, item: &Dict<'a>) -> Option<usize> {
        if let Some(dest) = item.get::<Object>(b"Dest") {
            return self.page_of(dest, 0);
        }
        let action = item.get::<Dict>(b"A")?;
        let kind = action.get::<Name>(b"S")?;
        if kind.as_ref() != b"GoTo" {
            return None;
        }
        self.page_of(action.get::<Object>(b"D")?, 0)
    }

    /// 宛先オブジェクト(配列・辞書・名前・文字列)が指すページ。
    fn page_of(&self, dest: Object<'a>, depth: usize) -> Option<usize> {
        if depth > MAX_TREE_DEPTH {
            return None;
        }
        match dest {
            Object::Array(array) => {
                let page_ref = array.raw_iter().next()?.as_obj_ref()?;
                self.pages.get(&ObjectIdentifier::from(page_ref)).copied()
            }
            Object::Dict(dict) => self.page_of(dict.get::<Object>(b"D")?, depth + 1),
            Object::Name(name) => self.named_destination(name.as_ref(), depth),
            Object::String(string) => self.named_destination(string.as_bytes(), depth),
            _ => None,
        }
    }

    /// 名前付き宛先を、カタログの`/Dests`辞書と`/Names`の`/Dests`ツリーから探す。
    fn named_destination(&self, key: &[u8], depth: usize) -> Option<usize> {
        if let Some(dests) = self.catalog.get::<Dict>(b"Dests")
            && let Some(dest) = dests.get::<Object>(key)
        {
            return self.page_of(dest, depth + 1);
        }
        let root = self.catalog.get::<Dict>(b"Names")?.get::<Dict>(b"Dests")?;
        let dest = find_in_name_tree(&root, key, 0)?;
        self.page_of(dest, depth + 1)
    }
}

/// ネームツリー(`/Names`の[キー, 値]の並び、または`/Kids`の子ノード)から`key`の値を探す。
fn find_in_name_tree<'a>(node: &Dict<'a>, key: &[u8], depth: usize) -> Option<Object<'a>> {
    if depth > MAX_TREE_DEPTH {
        return None;
    }
    if let Some(names) = node.get::<hayro::hayro_syntax::object::Array>(b"Names") {
        let mut iter = names.flex_iter();
        while let Some(name) = iter.next::<PdfString>() {
            let value = iter.next::<Object>()?;
            if name.as_bytes() == key {
                return Some(value);
            }
        }
    }
    let kids = node.get::<hayro::hayro_syntax::object::Array>(b"Kids")?;
    kids.iter::<Dict>()
        .find_map(|kid| find_in_name_tree(&kid, key, depth + 1))
}

/// PDFの「テキスト文字列」を`String`にする。
/// UTF-16BE(BOM付き)とUTF-8(BOM付き)を判別し、それ以外はPDFDocEncodingとして扱う
/// (Latin-1との差は一部の記号だけなので、Latin-1で近似する)。
fn decode_text_string(bytes: &[u8]) -> String {
    if let Some(utf16) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        let units = utf16
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_be_bytes(*pair));
        return char::decode_utf16(units)
            .map(|c| c.unwrap_or(char::REPLACEMENT_CHARACTER))
            .collect();
    }
    if let Some(utf8) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(utf8).into_owned();
    }
    bytes.iter().map(|&b| char::from(b)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_utf16be_with_bom() {
        assert_eq!(
            decode_text_string(&[0xFE, 0xFF, 0x7B, 0x2C, 0x00, 0x41]),
            "第A"
        );
    }

    #[test]
    fn decodes_utf8_with_bom() {
        assert_eq!(
            decode_text_string(&[0xEF, 0xBB, 0xBF, 0xE7, 0xAB, 0xA0]),
            "章"
        );
    }

    #[test]
    fn decodes_plain_bytes_as_latin1() {
        assert_eq!(decode_text_string(b"Chapter 1"), "Chapter 1");
        assert_eq!(decode_text_string(&[0x43, 0xE9]), "Cé");
    }

    #[test]
    fn broken_utf16_does_not_panic() {
        // 孤立したサロゲートと、奇数バイト
        assert_eq!(
            decode_text_string(&[0xFE, 0xFF, 0xD8, 0x00, 0x00, 0x41, 0x99]),
            "\u{FFFD}A"
        );
        assert_eq!(decode_text_string(&[]), "");
    }
}
