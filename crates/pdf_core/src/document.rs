use std::{fs, path::Path, sync::Arc};

use hayro::hayro_syntax::Pdf;

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
}
