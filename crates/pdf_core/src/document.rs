use std::{fs, path::Path, sync::Arc};

use hayro::hayro_syntax::{Pdf, page::Page};

use hayro::{
    PixmapSettings, RenderCache, RenderSettings, hayro_interpret::InterpreterSettings,
    vello_cpu::color::palette::css::WHITE,
};

use crate::{PdfError, RenderedPage};

pub struct PdfDocument {
    pdf: Pdf,
}

impl PdfDocument {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, PdfError> {
        let bytes = fs::read(path)?;
        let pdf = Pdf::new(Arc::new(bytes)).map_err(|e| PdfError::Load(format!("{e:?}")))?;
        Ok(Self { pdf })
    }

    pub fn page_count(&self) -> usize {
        self.pdf.pages().len()
    }

    /// ページのサイズ(幅, 高さ)をPDFポイント単位(1/72インチ)で返す。回転・CropBox反映済み。
    pub fn page_size(&self, index: usize) -> Result<(f32, f32), PdfError> {
        Ok(self.page(index)?.render_dimensions())
    }

    /// ページを`scale`倍(1.0 = 72dpi)でRGBAにラスタライズする。
    pub fn render_page(&self, index: usize, scale: f32) -> Result<RenderedPage, PdfError> {
        let page = self.page(index)?;
        let (w, h) = page.render_dimensions();
        if !scale.is_finite()
            || scale <= 0.0
            || w * scale >= f32::from(u16::MAX)
            || h * scale >= f32::from(u16::MAX)
        {
            return Err(PdfError::InvalidScale(scale));
        }
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
        // 背景が不透明な白なのでpremultipliedでもstraightでも値は同じ。
        Ok(RenderedPage {
            width: u32::from(pixmap.width()),
            height: u32::from(pixmap.height()),
            rgba: pixmap.data_as_u8_slice().to_vec(),
        })
    }

    fn page(&self, index: usize) -> Result<&Page<'_>, PdfError> {
        let pages = self.pdf.pages();
        pages.get(index).ok_or(PdfError::PageOutOfRange {
            index,
            count: pages.len(),
        })
    }
}
