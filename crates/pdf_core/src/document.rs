use std::{
    collections::HashMap,
    fs,
    num::NonZeroUsize,
    path::Path,
    sync::{Arc, Mutex, OnceLock},
};

use hayro::{
    PixmapSettings, RenderCache, RenderSettings,
    hayro_interpret::InterpreterSettings,
    hayro_syntax::{Pdf, object::ObjectIdentifier, page::Page},
    vello_cpu::color::palette::css::WHITE,
};
use lru::LruCache;

use crate::{
    Link, OutlineItem, PageText, PdfError, RenderedPage, dest::page_index_by_id, links::read_links,
    outline::read_outline, text::extract_text,
};

/// キャッシュするページ画像の既定の最大枚数。
const DEFAULT_CACHE_CAPACITY: usize = 16;

/// (ページ番号, 倍率のf32ビット列)。f32はHashできないのでビット列で代用する。
type CacheKey = (usize, u32);

pub struct PdfDocument {
    pdf: Pdf,
    cache: Mutex<LruCache<CacheKey, Arc<RenderedPage>>>,
    /// 目次。初めて必要になったときに読む。
    outline: OnceLock<Vec<OutlineItem>>,
    /// ページオブジェクトの識別子 → ページ番号。目次やリンクの宛先の解決に使う。
    page_ids: OnceLock<HashMap<ObjectIdentifier, usize>>,
    /// ページごとのリンク。そのページを初めて調べたときに読む。
    links: Vec<OnceLock<Vec<Link>>>,
    /// ページごとの文字。そのページを初めて調べたときに取り出す。
    texts: Vec<OnceLock<PageText>>,
}

impl PdfDocument {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, PdfError> {
        let bytes = fs::read(path)?;
        let pdf = Pdf::new(Arc::new(bytes)).map_err(|e| PdfError::Load(format!("{e:?}")))?;
        let page_count = pdf.pages().len();
        let capacity = NonZeroUsize::new(DEFAULT_CACHE_CAPACITY).unwrap();
        Ok(Self {
            pdf,
            cache: Mutex::new(LruCache::new(capacity)),
            outline: OnceLock::new(),
            page_ids: OnceLock::new(),
            links: (0..page_count).map(|_| OnceLock::new()).collect(),
            texts: (0..page_count).map(|_| OnceLock::new()).collect(),
        })
    }

    pub fn page_count(&self) -> usize {
        self.pdf.pages().len()
    }

    /// 文書の目次。無ければ空。初回だけ読み込む。
    pub fn outline(&self) -> &[OutlineItem] {
        self.outline
            .get_or_init(|| read_outline(&self.pdf, self.page_ids()))
    }

    /// ページのサイズ(幅, 高さ)をPDFポイント単位(1/72インチ)で返す。回転・CropBox反映済み。
    pub fn page_size(&self, index: usize) -> Result<(f32, f32), PdfError> {
        Ok(self.page(index)?.render_dimensions())
    }

    /// キャッシュに載せる最大枚数を変更する。超過分は古いものから捨てる。
    pub fn set_cache_capacity(&self, capacity: NonZeroUsize) {
        self.cache.lock().unwrap().resize(capacity);
    }

    /// ページを`scale`倍(1.0 = 72dpi)でRGBAにラスタライズする。同じ(ページ, 倍率)はキャッシュから返す。
    pub fn render_page(&self, index: usize, scale: f32) -> Result<Arc<RenderedPage>, PdfError> {
        let key = (index, scale.to_bits());
        if let Some(hit) = self.cache.lock().unwrap().get(&key) {
            return Ok(hit.clone());
        }
        // 描画は時間がかかるのでロックを外して行う。
        let rendered = Arc::new(self.rasterize(index, scale)?);
        self.cache.lock().unwrap().put(key, rendered.clone());
        Ok(rendered)
    }

    /// キャッシュを使わずに描画する。サムネイルのように大量に作る画像で、
    /// 本文用のキャッシュを押し出さないために使う。
    pub fn render_page_uncached(&self, index: usize, scale: f32) -> Result<RenderedPage, PdfError> {
        self.rasterize(index, scale)
    }

    fn rasterize(&self, index: usize, scale: f32) -> Result<RenderedPage, PdfError> {
        let page = self.page(index)?;
        let (w, h) = page.render_dimensions();
        validate_scale(scale, (w, h))?;
        let pixmap = hayro::render(
            page,
            &RenderCache::new(),
            &InterpreterSettings::default(),
            &RenderSettings::default(),
            &PixmapSettings {
                x_scale: scale,
                y_scale: scale,
                bg_color: WHITE,
            },
        );
        Ok(RenderedPage::from_pixmap(&pixmap))
    }

    /// ページ`index`のリンク。初回だけ読み込む。
    pub fn page_links(&self, index: usize) -> Result<&[Link], PdfError> {
        let cell = self.links.get(index).ok_or(PdfError::PageOutOfRange {
            index,
            count: self.links.len(),
        })?;
        Ok(cell.get_or_init(|| {
            let empty = PageText::default();
            let text = self.page_text(index).unwrap_or(&empty);
            read_links(&self.pdf, self.page_ids(), index, text)
        }))
    }

    /// ページ`index`のリンクが、すでに読み込み済みならそれを返す。読み込み中や未読み込みなら`None`で、
    /// 待たずに戻る(画面を更新するスレッドで、重い読み込みを待たないために使う)。
    pub fn page_links_if_ready(&self, index: usize) -> Option<&[Link]> {
        self.links.get(index)?.get().map(Vec::as_slice)
    }

    /// ページ`index`の文字が取り出し済みならそれを返す。未取り出しなら`None`で、待たずに戻る
    /// (画面を更新するスレッドで、重い取り出しを待たないために使う)。
    pub fn page_text_if_ready(&self, index: usize) -> Option<&PageText> {
        self.texts.get(index)?.get()
    }

    /// ページ`index`の文字と位置。初回だけ取り出す(ページの解釈が要るので、重いことがある)。
    pub fn page_text(&self, index: usize) -> Result<&PageText, PdfError> {
        let cell = self.texts.get(index).ok_or(PdfError::PageOutOfRange {
            index,
            count: self.texts.len(),
        })?;
        Ok(cell.get_or_init(|| {
            self.pdf
                .pages()
                .get(index)
                .map(extract_text)
                .unwrap_or_default()
        }))
    }

    fn page_ids(&self) -> &HashMap<ObjectIdentifier, usize> {
        self.page_ids.get_or_init(|| page_index_by_id(&self.pdf))
    }

    fn page(&self, index: usize) -> Result<&Page<'_>, PdfError> {
        let pages = self.pdf.pages();
        pages.get(index).ok_or(PdfError::PageOutOfRange {
            index,
            count: pages.len(),
        })
    }
}

/// 倍率が正の有限値で、描画後の画像サイズがhayroの上限(`u16`)に収まることを確かめる。
fn validate_scale(scale: f32, (width, height): (f32, f32)) -> Result<(), PdfError> {
    let max = f32::from(u16::MAX);
    if scale.is_finite() && scale > 0.0 && width * scale < max && height * scale < max {
        Ok(())
    } else {
        Err(PdfError::InvalidScale(scale))
    }
}
