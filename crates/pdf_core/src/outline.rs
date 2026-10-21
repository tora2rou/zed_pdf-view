//! PDFの目次(アウトライン、しおり)の読み取り。

use std::collections::{HashMap, HashSet};

use hayro::hayro_syntax::{
    Pdf,
    object::{Dict, ObjectIdentifier, String as PdfString},
};

use crate::dest::Destinations;

/// 読み込む項目数の上限。壊れた/悪意あるPDFで延々と読み続けないための安全弁。
const MAX_ITEMS: usize = 50_000;
/// 階層の深さの上限。
const MAX_DEPTH: usize = 32;

/// 目次の1項目。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutlineItem {
    pub title: String,
    /// 移動先のページ(0始まり)。リンク先が読めない・ページ外のときは`None`。
    pub page: Option<usize>,
    pub children: Vec<OutlineItem>,
}

/// 文書全体の目次を読む。目次が無い・読めない場合は空。
/// `page_ids`はページオブジェクトの識別子 → ページ番号の表。
pub(crate) fn read_outline(
    pdf: &Pdf,
    page_ids: &HashMap<ObjectIdentifier, usize>,
) -> Vec<OutlineItem> {
    let Some(dests) = Destinations::new(pdf, page_ids) else {
        return Vec::new();
    };
    let Some(outlines) = dests.catalog().get::<Dict>(b"Outlines") else {
        return Vec::new();
    };
    let mut reader = Reader {
        dests,
        budget: MAX_ITEMS,
        visited: HashSet::new(),
    };
    reader.read_siblings(outlines.get::<Dict>(b"First"), 0)
}

struct Reader<'a, 'p> {
    dests: Destinations<'a, 'p>,
    budget: usize,
    /// 読み込み済みの項目。循環を検出する。
    visited: HashSet<ObjectIdentifier>,
}

impl<'a> Reader<'a, '_> {
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
                page: self.dests.page_of_item(&dict),
                children,
            });
            current = dict.get::<Dict>(b"Next");
        }
        items
    }
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
