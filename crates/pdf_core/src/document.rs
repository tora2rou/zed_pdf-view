use std::{fs, path::Path, sync::Arc};

use hayro::hayro_syntax::{Pdf, page::Page};

use crate::PdfError;

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

    fn page(&self, index: usize) -> Result<&Page<'_>, PdfError> {
        let pages = self.pdf.pages();
        pages.get(index).ok_or(PdfError::PageOutOfRange {
            index,
            count: pages.len(),
        })
    }
}
