//! 表示中のページのOCR(画像だけのPDFの文字を選択・コピーできるようにする)。

use std::sync::Arc;

use gpui::{AppContext as _, Context, Window};

use crate::{actions::RunOcr, view::PdfView};

impl PdfView {
    /// 現在のページをバックグラウンドでOCRする。結果はそのページの文字になり、選択・コピーに使える。
    pub(crate) fn run_ocr(&mut self, _: &RunOcr, window: &mut Window, cx: &mut Context<Self>) {
        if self.ocr_running {
            return;
        }
        let (w, h) = self.viewport_size(window);
        let index = self.current_page(&self.layout(w), h);
        if self.document.is_ocr_done(index) {
            self.show_notice("This page has already been OCRed", window, cx);
            return;
        }
        self.ocr_running = true;
        self.show_notice("Running OCR…", window, cx);
        let document = Arc::clone(&self.document);
        cx.spawn_in(window, async move |this, cx| {
            let target = Arc::clone(&document);
            let result = cx
                .background_spawn(async move { target.ocr_page(index).map(|t| t.chars.len()) })
                .await;
            this.update_in(cx, |this, window, cx| {
                // OCRの間に別の文書へ差し替えられていたら、結果は見せない。
                if !Arc::ptr_eq(&this.document, &document) {
                    return;
                }
                this.ocr_running = false;
                let message = match result {
                    Ok(0) => "OCR found no text on this page".to_string(),
                    Ok(count) => format!("OCR done ({count} characters). Use Select (V) to copy"),
                    Err(e) => e.to_string(),
                };
                this.show_notice(&message, window, cx);
            })
            .ok();
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use gpui::TestAppContext;
    use pdf_core::PdfDocument;

    use crate::{actions::RunOcr, view::PdfView};

    const FIXTURE: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../pdf_core/tests/fixtures/two_pages.pdf"
    );

    /// OCRが終わる(Tesseractが無くて失敗する場合も含む)と、実行中の表示が消えてお知らせが出る。
    #[gpui::test]
    fn ocr_finishes_with_a_notice(cx: &mut TestAppContext) {
        let document = Arc::new(PdfDocument::open(FIXTURE).unwrap());
        let (view, cx) = cx.add_window_view(|_, cx| PdfView::new(document, cx));
        view.update_in(cx, |this, window, cx| this.run_ocr(&RunOcr, window, cx));
        assert!(view.read_with(cx, |this, _| this.ocr_running));

        cx.executor().allow_parking();
        for _ in 0..200 {
            cx.run_until_parked();
            if !view.read_with(cx, |this, _| this.ocr_running) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let (running, notice) =
            view.read_with(cx, |this, _| (this.ocr_running, this.notice.clone()));
        assert!(!running, "OCR never finished");
        assert!(notice.is_some_and(|n| n.as_ref() != "Running OCR…"));
    }
}
