use std::{
    fs,
    num::NonZeroUsize,
    path::Path,
    sync::{Arc, Mutex},
};

use hayro::{
    PixmapSettings, RenderCache, RenderSettings,
    hayro_interpret::InterpreterSettings,
    hayro_syntax::{Pdf, page::Page},
    vello_cpu::color::palette::css::WHITE,
};
use lru::LruCache;

use crate::{PdfError, RenderedPage};

/// キャッシュするページ画像の既定の最大枚数。
const DEFAULT_CACHE_CAPACITY: usize = 16;

/// (ページ番号, 倍率のf32ビット列)。f32はHashできないのでビット列で代用する。
type CacheKey = (usize, u32);

pub struct PdfDocument {
    pdf: Pdf,
    cache: Mutex<LruCache<CacheKey, Arc<RenderedPage>>>,
}

impl PdfDocument {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, PdfError> {
        let bytes = fs::read(path)?;
        let pdf = Pdf::new(Arc::new(bytes)).map_err(|e| PdfError::Load(format!("{e:?}")))?;
        let capacity = NonZeroUsize::new(DEFAULT_CACHE_CAPACITY).unwrap();
        Ok(Self {
            pdf,
            cache: Mutex::new(LruCache::new(capacity)),
        })
    }

    pub fn page_count(&self) -> usize {
        self.pdf.pages().len()
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
