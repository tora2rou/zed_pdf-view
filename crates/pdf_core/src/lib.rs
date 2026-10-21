//! PDFの読み込み・ラスタライズ・キャッシュを行う、GPUI非依存のコア。

mod document;
mod error;

pub use document::PdfDocument;
pub use error::PdfError;
