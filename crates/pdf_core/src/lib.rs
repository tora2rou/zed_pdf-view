//! PDFの読み込み・ラスタライズ・キャッシュを行う、GPUI非依存のコア。

mod dest;
mod document;
mod error;
mod linkify;
mod links;
mod outline;
mod render;
mod text;

pub use document::PdfDocument;
pub use error::PdfError;
pub use links::{Link, LinkRect, LinkTarget};
pub use outline::OutlineItem;
pub use render::RenderedPage;
pub use text::{Gap, PageText, TextChar};
