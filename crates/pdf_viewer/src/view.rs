use std::sync::Arc;

use gpui::{
    App, Context, FocusHandle, Focusable, InteractiveElement, IntoElement, ParentElement, Render,
    RenderImage, Styled, Window, div, img, px, rgb,
};
use pdf_core::{PdfDocument, PdfError};

use crate::to_render_image;

/// 現在表示中のページ画像。(ページ, 描画倍率)が変わるまで使い回す。
struct PageImage {
    key: (usize, u32),
    image: Arc<RenderImage>,
}

/// PDFを表示するビュー。Zed本体の`image_viewer`のようにItemとして組み込めるよう、
/// GPUIの`Render`/`Focusable`だけに依存し、ファイルの読み込みは呼び出し側が行う。
pub struct PdfView {
    document: Arc<PdfDocument>,
    page_index: usize,
    zoom: f32,
    focus_handle: FocusHandle,
    current: Option<PageImage>,
}

impl PdfView {
    pub fn new(document: Arc<PdfDocument>, cx: &mut Context<Self>) -> Self {
        Self {
            document,
            page_index: 0,
            zoom: 1.0,
            focus_handle: cx.focus_handle(),
            current: None,
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

    /// 現在のページ・ズーム・画面のDPIに合った画像を返す。変わっていれば描画し直す。
    fn page_image(&mut self, window: &mut Window) -> Result<Arc<RenderImage>, PdfError> {
        let scale = self.zoom * window.scale_factor();
        let key = (self.page_index, scale.to_bits());
        if let Some(current) = self.current.as_ref().filter(|c| c.key == key) {
            return Ok(current.image.clone());
        }
        let rendered = self.document.render_page(self.page_index, scale)?;
        let image = to_render_image(&rendered)
            .ok_or_else(|| PdfError::Load("画像データのサイズが不正です".into()))?;
        if let Some(old) = self.current.replace(PageImage {
            key,
            image: image.clone(),
        }) {
            // 古い画像がGPUのアトラスに残り続けないようにする。
            window.drop_image(old.image).ok();
        }
        Ok(image)
    }
}

impl Render for PdfView {
    fn render(&mut self, window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let root = div()
            .track_focus(&self.focus_handle)
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgb(0x3c3c3c));
        let size = self.document.page_size(self.page_index);
        match (self.page_image(window), size) {
            (Ok(image), Ok((w, h))) => {
                root.child(img(image).w(px(w * self.zoom)).h(px(h * self.zoom)))
            }
            (Err(e), _) | (_, Err(e)) => root.text_color(rgb(0xff6666)).child(e.to_string()),
        }
    }
}

impl Focusable for PdfView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}
