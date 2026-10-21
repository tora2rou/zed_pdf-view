use std::sync::Arc;

use gpui::{App, Context, FocusHandle, Focusable};
use pdf_core::PdfDocument;

/// PDFを表示するビュー。Zed本体の`image_viewer`のようにItemとして組み込めるよう、
/// GPUIの`Render`/`Focusable`だけに依存し、ファイルの読み込みは呼び出し側が行う。
pub struct PdfView {
    document: Arc<PdfDocument>,
    page_index: usize,
    zoom: f32,
    focus_handle: FocusHandle,
}

impl PdfView {
    pub fn new(document: Arc<PdfDocument>, cx: &mut Context<Self>) -> Self {
        Self {
            document,
            page_index: 0,
            zoom: 1.0,
            focus_handle: cx.focus_handle(),
        }
    }

    pub fn document(&self) -> &Arc<PdfDocument> {
        &self.document
    }

    pub fn page_index(&self) -> usize {
        self.page_index
    }

    pub fn zoom(&self) -> f32 {
        self.zoom
    }
}

impl Focusable for PdfView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}
