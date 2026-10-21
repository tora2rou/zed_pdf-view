//! リンクと目次に共通の、宛先(ページへの移動先)の解決。

use std::collections::HashMap;

use hayro::hayro_syntax::{
    Pdf,
    object::{Array, Dict, Name, Object, ObjectIdentifier, String as PdfString},
};

/// 名前付き宛先のツリーをたどる深さの上限。
const MAX_TREE_DEPTH: usize = 16;

/// ページオブジェクトの識別子 → ページ番号(0始まり)の表を作る。
pub(crate) fn page_index_by_id(pdf: &Pdf) -> HashMap<ObjectIdentifier, usize> {
    pdf.pages()
        .iter()
        .enumerate()
        .filter_map(|(index, page)| Some((page.raw().obj_id()?, index)))
        .collect()
}

/// 宛先をページ番号に解決する。名前付き宛先のために、文書のカタログを持つ。
pub(crate) struct Destinations<'a, 'p> {
    catalog: Dict<'a>,
    page_ids: &'p HashMap<ObjectIdentifier, usize>,
}

impl<'a, 'p> Destinations<'a, 'p> {
    /// カタログが読めない壊れたPDFでは`None`。
    pub(crate) fn new(
        pdf: &'a Pdf,
        page_ids: &'p HashMap<ObjectIdentifier, usize>,
    ) -> Option<Self> {
        let xref = pdf.xref();
        Some(Self {
            catalog: xref.get::<Dict>(xref.root_id())?,
            page_ids,
        })
    }

    pub(crate) fn catalog(&self) -> &Dict<'a> {
        &self.catalog
    }

    /// 目次の項目やリンク注釈の移動先ページ。`/Dest`、または`/A`(GoToアクション)の`/D`から求める。
    pub(crate) fn page_of_item(&self, item: &Dict<'a>) -> Option<usize> {
        if let Some(dest) = item.get::<Object>(b"Dest") {
            return self.page_of(dest, 0);
        }
        self.page_of_action(&item.get::<Dict>(b"A")?)
    }

    /// GoToアクションの移動先ページ。GoTo以外のアクションは`None`。
    pub(crate) fn page_of_action(&self, action: &Dict<'a>) -> Option<usize> {
        let kind = action.get::<Name>(b"S")?;
        if kind.as_ref() != b"GoTo" {
            return None;
        }
        self.page_of(action.get::<Object>(b"D")?, 0)
    }

    /// 宛先オブジェクト(配列・辞書・名前・文字列)が指すページ。
    pub(crate) fn page_of(&self, dest: Object<'a>, depth: usize) -> Option<usize> {
        if depth > MAX_TREE_DEPTH {
            return None;
        }
        match dest {
            Object::Array(array) => {
                let page_ref = array.raw_iter().next()?.as_obj_ref()?;
                self.page_ids
                    .get(&ObjectIdentifier::from(page_ref))
                    .copied()
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
    if let Some(names) = node.get::<Array>(b"Names") {
        let mut iter = names.flex_iter();
        while let Some(name) = iter.next::<PdfString>() {
            let value = iter.next::<Object>()?;
            if name.as_bytes() == key {
                return Some(value);
            }
        }
    }
    let kids = node.get::<Array>(b"Kids")?;
    kids.iter::<Dict>()
        .find_map(|kid| find_in_name_tree(&kid, key, depth + 1))
}
