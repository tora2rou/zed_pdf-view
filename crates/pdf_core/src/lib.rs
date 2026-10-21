//! PDFの読み込み・ラスタライズ・キャッシュを行う、GPUI非依存のコア。

mod dest;
mod document;
mod error;
mod links;
mod outline;
mod render;

pub use document::PdfDocument;
pub use error::PdfError;
pub use links::{Link, LinkRect, LinkTarget};
pub use outline::OutlineItem;
pub use render::RenderedPage;
