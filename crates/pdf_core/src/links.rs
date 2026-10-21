//! ページ上のリンク注釈(`/Link`)の読み取り。

use std::collections::HashMap;

use hayro::hayro_syntax::{
    Pdf,
    object::{Array, Dict, Name, Object, ObjectIdentifier, String as PdfString},
};

use crate::{dest::Destinations, linkify::detect_urls, text::PageText};

/// 1ページから読むリンク数の上限。壊れた/悪意あるPDFへの安全弁。
const MAX_LINKS_PER_PAGE: usize = 10_000;
/// URIの長さの上限。
const MAX_URI_LEN: usize = 4096;
/// 注釈フラグ: 非表示(Hidden)と、画面に表示しない(NoView)。
const FLAG_HIDDEN: i32 = 1 << 1;
const FLAG_NO_VIEW: i32 = 1 << 5;

/// リンクの範囲。描画された画像(回転・クロップボックス反映済み)と同じ座標系で、
/// 倍率1.0・原点は左上・単位はポイント。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LinkRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl LinkRect {
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.x + self.width && y >= self.y && y < self.y + self.height
    }
}

/// リンクの移動先。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkTarget {
    /// 文書内のページ(0始まり)。
    Page(usize),
    /// 外部のURI。どのスキームでも読み取ったまま返すので、開く側で確かめる。
    Uri(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Link {
    pub rect: LinkRect,
    pub target: LinkTarget,
}

/// ページ`index`のリンクを読む。リンク注釈をPDFの記述順に、続けて、文字の中のURLのうち
/// 注釈と重ならないものを並べる。読めないものは飛ばす。`text`はそのページの文字。
pub(crate) fn read_links(
    pdf: &Pdf,
    page_ids: &HashMap<ObjectIdentifier, usize>,
    index: usize,
    text: &PageText,
) -> Vec<Link> {
    let mut links = read_annotation_links(pdf, page_ids, index);
    for detected in detect_urls(text) {
        // 注釈が付いている箇所は、注釈を優先する(同じURLを二重に持たない)。
        let (cx, cy) = (
            detected.rect.x + detected.rect.width / 2.0,
            detected.rect.y + detected.rect.height / 2.0,
        );
        if !links
            .iter()
            .any(|annotated| annotated.rect.contains(cx, cy))
        {
            links.push(detected);
        }
    }
    links
}

/// ページ`index`のリンク注釈を、PDFの記述順に読む。
fn read_annotation_links(
    pdf: &Pdf,
    page_ids: &HashMap<ObjectIdentifier, usize>,
    index: usize,
) -> Vec<Link> {
    let pages = pdf.pages();
    let Some(page) = pages.get(index) else {
        return Vec::new();
    };
    let Some(annots) = page.raw().get::<Array>(b"Annots") else {
        return Vec::new();
    };
    let to_display = page.initial_transform(true).as_coeffs();
    let dests = Destinations::new(pdf, page_ids);

    annots
        .iter::<Dict>()
        .take(MAX_LINKS_PER_PAGE)
        .filter_map(|annot| {
            if annot.get::<Name>(b"Subtype")?.as_ref() != b"Link" {
                return None;
            }
            if annot.get::<i32>(b"F").unwrap_or(0) & (FLAG_HIDDEN | FLAG_NO_VIEW) != 0 {
                return None;
            }
            let rect = display_rect(annot.get::<[f64; 4]>(b"Rect")?, to_display)?;
            let target = target_of(&annot, dests.as_ref())?;
            Some(Link { rect, target })
        })
        .collect()
}

/// リンク注釈の移動先。`/Dest`、または`/A`のGoTo・URIアクション。
fn target_of(annot: &Dict<'_>, dests: Option<&Destinations<'_, '_>>) -> Option<LinkTarget> {
    if let Some(dest) = annot.get::<Object>(b"Dest") {
        return Some(LinkTarget::Page(dests?.page_of(dest, 0)?));
    }
    let action = annot.get::<Dict>(b"A")?;
    match action.get::<Name>(b"S")?.as_ref() {
        b"GoTo" => Some(LinkTarget::Page(dests?.page_of_action(&action)?)),
        b"URI" => {
            let uri = action.get::<PdfString>(b"URI")?;
            let uri = String::from_utf8_lossy(uri.as_bytes()).trim().to_string();
            (!uri.is_empty() && uri.len() <= MAX_URI_LEN).then_some(LinkTarget::Uri(uri))
        }
        _ => None,
    }
}

/// ユーザー空間の矩形`[x0 y0 x1 y1]`(向きは不定)を、表示座標の外接矩形にする。
/// `to_display`は`[a b c d e f]`(x' = a·x + c·y + e, y' = b·x + d·y + f)。
fn display_rect(rect: [f64; 4], to_display: [f64; 6]) -> Option<LinkRect> {
    let [a, b, c, d, e, f] = to_display;
    // f64::minは片方がNaNだともう片方を返すので、NaNは計算の前に弾く。
    if rect.iter().any(|v| !v.is_finite()) {
        return None;
    }
    let [x0, y0, x1, y1] = rect;
    let corners = [(x0, y0), (x1, y0), (x0, y1), (x1, y1)];
    let mut min = (f64::INFINITY, f64::INFINITY);
    let mut max = (f64::NEG_INFINITY, f64::NEG_INFINITY);
    for (x, y) in corners {
        let (tx, ty) = (a * x + c * y + e, b * x + d * y + f);
        min = (min.0.min(tx), min.1.min(ty));
        max = (max.0.max(tx), max.1.max(ty));
    }
    let values = [min.0, min.1, max.0, max.1];
    if values.iter().any(|v| !v.is_finite()) {
        return None;
    }
    Some(LinkRect {
        x: min.0 as f32,
        y: min.1 as f32,
        width: (max.0 - min.0) as f32,
        height: (max.1 - min.1) as f32,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 恒等 + y反転(高さ100): ユーザー空間の(x, y)が表示の(x, 100 - y)になる。
    const FLIP_100: [f64; 6] = [1.0, 0.0, 0.0, -1.0, 0.0, 100.0];

    #[test]
    fn display_rect_flips_y() {
        let r = display_rect([10.0, 10.0, 60.0, 30.0], FLIP_100).unwrap();
        assert_eq!(
            r,
            LinkRect {
                x: 10.0,
                y: 70.0,
                width: 50.0,
                height: 20.0
            }
        );
    }

    #[test]
    fn display_rect_accepts_corners_in_any_order() {
        let a = display_rect([60.0, 30.0, 10.0, 10.0], FLIP_100).unwrap();
        let b = display_rect([10.0, 10.0, 60.0, 30.0], FLIP_100).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn display_rect_rejects_non_finite_values() {
        assert!(display_rect([f64::NAN, 0.0, 1.0, 1.0], FLIP_100).is_none());
    }

    #[test]
    fn contains_is_half_open() {
        let r = LinkRect {
            x: 10.0,
            y: 20.0,
            width: 30.0,
            height: 40.0,
        };
        assert!(r.contains(10.0, 20.0));
        assert!(r.contains(39.9, 59.9));
        assert!(!r.contains(40.0, 30.0));
        assert!(!r.contains(20.0, 60.0));
        assert!(!r.contains(9.9, 30.0));
    }
}
