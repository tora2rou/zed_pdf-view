//! ページ画像の保持と、描画要求の管理。GPUIの`Window`には依存しない(テストしやすくするため)。
//!
//! 流れ: 表示したいページを`lookup`で調べる → 無ければ`request`で要求を登録して
//! バックグラウンドで描画する → 終わったら`complete`で受け取る。
//! 描画中は、同じ回転の古い画像があればそれを拡大縮小して見せる。

use std::{collections::HashMap, ops::Range, sync::Arc, time::Duration};

use gpui::RenderImage;
use pdf_core::{PdfDocument, PdfError};

use crate::{
    to_render_image,
    transform::{Appearance, transform},
};

/// 画像を作り直す必要があるかを決める条件。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageKey {
    /// 描画倍率(`f32`のビット列)。
    pub scale_bits: u32,
    pub appearance: Appearance,
    /// 文書を入れ替えるたびに増える。古い文書の描画結果を受け取らないため。
    pub generation: u64,
}

/// `lookup`の結果。
#[derive(Debug, Clone)]
pub enum Lookup {
    /// 条件にぴったり合う画像がある。
    Fresh(Arc<RenderImage>),
    /// 倍率などが古いが、拡大縮小すれば仮に表示できる画像がある。
    Stale(Arc<RenderImage>),
    /// 描画に失敗した。
    Failed(Arc<str>),
    /// 何も無い。
    Missing,
}

struct PageImage {
    key: ImageKey,
    image: Arc<RenderImage>,
}

/// ページごとの画像と、進行中の描画要求。
#[derive(Default)]
pub struct PageImages {
    ready: HashMap<usize, PageImage>,
    failed: HashMap<usize, (ImageKey, Arc<str>)>,
    /// ページごとの最新の描画要求。これと違う`key`の結果は捨てる。
    wanted: HashMap<usize, ImageKey>,
    generation: u64,
}

impl PageImages {
    /// いまの世代(文書の入れ替えごとに変わる)を含めた`ImageKey`を作る。
    pub fn key(&self, scale: f32, appearance: Appearance) -> ImageKey {
        ImageKey {
            scale_bits: scale.to_bits(),
            appearance,
            generation: self.generation,
        }
    }

    pub fn lookup(&self, index: usize, key: &ImageKey) -> Lookup {
        if let Some((failed_key, message)) = self.failed.get(&index)
            && failed_key == key
        {
            return Lookup::Failed(message.clone());
        }
        match self.ready.get(&index) {
            Some(page) if page.key == *key => Lookup::Fresh(page.image.clone()),
            // 回転が違うと縦横比が変わって歪むので、仮表示には使わない。
            Some(page)
                if page.key.generation == key.generation
                    && page.key.appearance.rotation == key.appearance.rotation =>
            {
                Lookup::Stale(page.image.clone())
            }
            _ => Lookup::Missing,
        }
    }

    /// `index`の描画を要求済みにする。新しく描画を始めるべきなら`true`
    /// (すでに同じ条件で要求済みなら`false`)。
    pub fn request(&mut self, index: usize, key: ImageKey) -> bool {
        if self.wanted.get(&index) == Some(&key) {
            return false;
        }
        self.wanted.insert(index, key);
        true
    }

    /// 描画結果を受け取る。最新の要求と違う条件の結果は捨てる。
    /// 置き換わった古い画像を返すので、呼び出し側でGPUから破棄する。
    pub fn complete(
        &mut self,
        index: usize,
        key: ImageKey,
        result: Result<Arc<RenderImage>, PdfError>,
    ) -> Option<Arc<RenderImage>> {
        if self.wanted.get(&index) != Some(&key) {
            return result.ok();
        }
        match result {
            Ok(image) => {
                self.failed.remove(&index);
                self.ready
                    .insert(index, PageImage { key, image })
                    .map(|old| old.image)
            }
            Err(e) => {
                self.failed.insert(index, (key, e.to_string().into()));
                None
            }
        }
    }

    /// `keep`の範囲外のページの画像と要求を捨て、GPUから破棄すべき画像を返す。
    pub fn retain(&mut self, keep: &Range<usize>) -> Vec<Arc<RenderImage>> {
        self.wanted.retain(|index, _| keep.contains(index));
        self.failed.retain(|index, _| keep.contains(index));
        let dropped: Vec<usize> = self
            .ready
            .keys()
            .copied()
            .filter(|index| !keep.contains(index))
            .collect();
        dropped
            .into_iter()
            .filter_map(|index| self.ready.remove(&index).map(|page| page.image))
            .collect()
    }

    /// すべて捨てる(文書を入れ替えるとき)。世代を進めるので、進行中の描画結果は受け取られない。
    pub fn clear(&mut self) -> Vec<Arc<RenderImage>> {
        self.generation += 1;
        self.wanted.clear();
        self.failed.clear();
        self.ready.drain().map(|(_, page)| page.image).collect()
    }
}

/// 画像を作るときに、文書の描画キャッシュを使うか。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderMode {
    /// 本文用。文書のLRUキャッシュに載せる(回転や反転の切り替えで再利用できる)。
    Cached,
    /// サムネイル用。大量に作るので、本文用のキャッシュを押し出さないよう載せない。
    Uncached,
}

/// ページ`index`を描画して、GPUに載せられる画像にする。重いのでバックグラウンドで呼ぶ。
pub fn render_page_image(
    document: &PdfDocument,
    index: usize,
    scale: f32,
    appearance: Appearance,
    mode: RenderMode,
) -> Result<Arc<RenderImage>, PdfError> {
    let rendered = match mode {
        RenderMode::Cached => document.render_page(index, scale)?,
        RenderMode::Uncached => Arc::new(document.render_page_uncached(index, scale)?),
    };
    to_render_image(&transform(&rendered, appearance))
        .ok_or_else(|| PdfError::Load("画像データのサイズが不正です".into()))
}

/// ズーム直後に再描画を待つ時間(連続操作の途中で無駄な描画をしないため)。
pub const ZOOM_SETTLE: Duration = Duration::from_millis(120);

/// 最後のズーム操作から`elapsed`たったとき、あと何ミリ秒待つべきか。待たなくてよければ`None`。
pub fn settle_remaining(elapsed: Duration) -> Option<Duration> {
    ZOOM_SETTLE.checked_sub(elapsed).filter(|d| !d.is_zero())
}

/// 表示範囲の前後に先読みするページ数。
pub const PREFETCH_PAGES: usize = 1;

/// 表示中の範囲`visible`を、前後に`margin`ページずつ広げた範囲(`page_count`を超えない)。
pub fn with_margin(visible: &Range<usize>, margin: usize, page_count: usize) -> Range<usize> {
    if visible.is_empty() {
        return visible.clone();
    }
    visible.start.saturating_sub(margin)..(visible.end + margin).min(page_count)
}

/// 本文用: 表示中の範囲を先読み分だけ広げた範囲。
pub fn with_prefetch(visible: &Range<usize>, page_count: usize) -> Range<usize> {
    with_margin(visible, PREFETCH_PAGES, page_count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transform::Rotation;
    use pdf_core::RenderedPage;

    fn image() -> Arc<RenderImage> {
        let page = RenderedPage {
            width: 1,
            height: 1,
            rgba: vec![0; 4],
        };
        to_render_image(&page).unwrap()
    }

    fn appearance(rotation: Rotation) -> Appearance {
        Appearance {
            rotation,
            invert: false,
        }
    }

    #[test]
    fn missing_until_completed() {
        let mut images = PageImages::default();
        let key = images.key(1.0, Appearance::default());
        assert!(matches!(images.lookup(0, &key), Lookup::Missing));
        assert!(images.request(0, key));
        images.complete(0, key, Ok(image()));
        assert!(matches!(images.lookup(0, &key), Lookup::Fresh(_)));
    }

    #[test]
    fn duplicate_requests_are_ignored() {
        let mut images = PageImages::default();
        let key = images.key(1.0, Appearance::default());
        assert!(images.request(0, key));
        assert!(!images.request(0, key));
        let other = images.key(2.0, Appearance::default());
        assert!(images.request(0, other));
    }

    #[test]
    fn old_scale_is_shown_as_stale_while_rendering() {
        let mut images = PageImages::default();
        let old = images.key(1.0, Appearance::default());
        images.request(0, old);
        images.complete(0, old, Ok(image()));
        let new = images.key(2.0, Appearance::default());
        assert!(matches!(images.lookup(0, &new), Lookup::Stale(_)));
    }

    #[test]
    fn rotated_image_is_not_used_as_stale() {
        let mut images = PageImages::default();
        let old = images.key(1.0, Appearance::default());
        images.request(0, old);
        images.complete(0, old, Ok(image()));
        let rotated = images.key(1.0, appearance(Rotation::Deg90));
        assert!(matches!(images.lookup(0, &rotated), Lookup::Missing));
    }

    #[test]
    fn inverted_image_is_used_as_stale() {
        let mut images = PageImages::default();
        let old = images.key(1.0, Appearance::default());
        images.request(0, old);
        images.complete(0, old, Ok(image()));
        let dark = images.key(
            1.0,
            Appearance {
                invert: true,
                ..Default::default()
            },
        );
        assert!(matches!(images.lookup(0, &dark), Lookup::Stale(_)));
    }

    #[test]
    fn superseded_result_is_discarded() {
        let mut images = PageImages::default();
        let first = images.key(1.0, Appearance::default());
        let second = images.key(2.0, Appearance::default());
        images.request(0, first);
        images.request(0, second); // ズームが変わり、最初の要求は古くなった
        // 古い要求の結果が届いても、保持しない(戻り値でGPU破棄を促す)
        assert!(images.complete(0, first, Ok(image())).is_some());
        assert!(matches!(images.lookup(0, &first), Lookup::Missing));
        images.complete(0, second, Ok(image()));
        assert!(matches!(images.lookup(0, &second), Lookup::Fresh(_)));
    }

    #[test]
    fn replacing_an_image_returns_the_old_one_to_drop() {
        let mut images = PageImages::default();
        let a = images.key(1.0, Appearance::default());
        images.request(0, a);
        assert!(images.complete(0, a, Ok(image())).is_none());
        let b = images.key(2.0, Appearance::default());
        images.request(0, b);
        assert!(images.complete(0, b, Ok(image())).is_some());
    }

    #[test]
    fn failure_is_remembered_for_the_same_key_only() {
        let mut images = PageImages::default();
        let key = images.key(1.0, Appearance::default());
        images.request(0, key);
        images.complete(0, key, Err(PdfError::InvalidScale(0.0)));
        assert!(matches!(images.lookup(0, &key), Lookup::Failed(_)));
        let other = images.key(2.0, Appearance::default());
        assert!(matches!(images.lookup(0, &other), Lookup::Missing));
    }

    #[test]
    fn retain_drops_pages_outside_the_range() {
        let mut images = PageImages::default();
        let key = images.key(1.0, Appearance::default());
        for i in 0..4 {
            images.request(i, key);
            images.complete(i, key, Ok(image()));
        }
        let dropped = images.retain(&(1..3));
        assert_eq!(dropped.len(), 2);
        assert!(matches!(images.lookup(0, &key), Lookup::Missing));
        assert!(matches!(images.lookup(1, &key), Lookup::Fresh(_)));
        // 範囲外になった要求は取り下げられ、戻ってきたら改めて要求できる
        assert!(images.request(0, key));
    }

    #[test]
    fn clearing_rejects_results_from_the_previous_document() {
        let mut images = PageImages::default();
        let old = images.key(1.0, Appearance::default());
        images.request(0, old);
        images.clear();
        // 新しい文書で同じ倍率・同じページを要求しても、世代が違うので古い結果は受け取らない
        let new = images.key(1.0, Appearance::default());
        assert_ne!(old, new);
        images.request(0, new);
        assert!(images.complete(0, old, Ok(image())).is_some());
        assert!(matches!(images.lookup(0, &new), Lookup::Missing));
    }

    #[test]
    fn settle_waits_only_while_recent() {
        assert_eq!(
            settle_remaining(Duration::from_millis(20)),
            Some(Duration::from_millis(100))
        );
        assert_eq!(settle_remaining(ZOOM_SETTLE), None);
        assert_eq!(settle_remaining(Duration::from_secs(1)), None);
    }

    #[test]
    fn margin_extends_the_range_within_bounds() {
        assert_eq!(with_margin(&(10..12), 4, 100), 6..16);
        assert_eq!(with_margin(&(1..3), 4, 100), 0..7);
        assert_eq!(with_margin(&(98..100), 4, 100), 94..100);
        assert_eq!(with_margin(&(5..5), 4, 100), 5..5);
    }

    #[test]
    fn prefetch_extends_the_range_within_bounds() {
        assert_eq!(with_prefetch(&(3..5), 10), 2..6);
        assert_eq!(with_prefetch(&(0..2), 10), 0..3);
        assert_eq!(with_prefetch(&(8..10), 10), 7..10);
        assert_eq!(with_prefetch(&(0..0), 10), 0..0);
    }
}
