//! OCR(画像だけのPDFの文字を選択・コピーできるようにする)。現在のページか全ページを選んで実行する。

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use gpui::{
    AppContext as _, ClickEvent, Context, MouseDownEvent, Stateful, Window, deferred, div,
    prelude::*, px, rgb,
};

use crate::{
    actions::{CancelOcr, RunOcr, RunOcrAllPages},
    toolbar::{TOOLBAR_HEIGHT, button},
    view::PdfView,
};

const MENU_BACKGROUND: u32 = 0x1e1e1e;
const MENU_BORDER: u32 = 0x555555;
const MENU_HOVER: u32 = 0x3a3a3a;
const MENU_WIDTH: f32 = 120.0;

impl PdfView {
    /// 現在のページをOCRする。
    pub(crate) fn run_ocr(&mut self, _: &RunOcr, window: &mut Window, cx: &mut Context<Self>) {
        self.ocr_menu_open = false;
        let (w, h) = self.viewport_size(window);
        let index = self.current_page(&self.layout(w), h);
        self.start_ocr(vec![index], window, cx);
    }

    /// 全ページをOCRする。
    pub(crate) fn run_ocr_all_pages(
        &mut self,
        _: &RunOcrAllPages,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ocr_menu_open = false;
        let pages = (0..self.document.page_count()).collect();
        self.start_ocr(pages, window, cx);
    }

    /// 実行中のOCRを、いま処理中のページが終わったところで止める。
    pub(crate) fn cancel_ocr(&mut self, _: &CancelOcr, _: &mut Window, _: &mut Context<Self>) {
        self.ocr_cancel.store(true, Ordering::Relaxed);
    }

    /// `pages`のうちOCR済みでないものを、順にバックグラウンドでOCRする。結果はそのページの文字になり、
    /// 選択・コピーに使える。進み具合と結果はお知らせで伝える。
    fn start_ocr(&mut self, pages: Vec<usize>, window: &mut Window, cx: &mut Context<Self>) {
        if self.ocr_running {
            return;
        }
        let single = pages.len() == 1;
        let pages: Vec<usize> = pages
            .into_iter()
            .filter(|&i| !self.document.is_ocr_done(i))
            .collect();
        if pages.is_empty() {
            let message = if single {
                "This page has already been OCRed"
            } else {
                "All pages have already been OCRed"
            };
            self.show_notice(message, window, cx);
            return;
        }
        self.ocr_running = true;
        let cancel = Arc::new(AtomicBool::new(false));
        self.ocr_cancel = Arc::clone(&cancel);
        self.show_notice("Running OCR…", window, cx);
        let document = Arc::clone(&self.document);
        cx.spawn_in(window, async move |this, cx| {
            let total = pages.len();
            let mut characters = 0;
            let mut finished = 0;
            let mut failure = None;
            for (n, index) in pages.into_iter().enumerate() {
                if cancel.load(Ordering::Relaxed) {
                    break;
                }
                if total > 1 {
                    let shown = this.update_in(cx, |this, window, cx| {
                        // OCRの間に別の文書へ差し替えられていたら、そこで終わる。
                        let current = Arc::ptr_eq(&this.document, &document);
                        if current {
                            this.show_notice(&format!("Running OCR… {}/{total}", n + 1), window, cx);
                        }
                        current
                    });
                    if !matches!(shown, Ok(true)) {
                        return;
                    }
                }
                let target = Arc::clone(&document);
                match cx
                    .background_spawn(async move { target.ocr_page(index).map(|t| t.chars.len()) })
                    .await
                {
                    Ok(count) => {
                        characters += count;
                        finished += 1;
                    }
                    Err(e) => {
                        failure = Some(e.to_string());
                        break;
                    }
                }
            }
            let cancelled = cancel.load(Ordering::Relaxed);
            this.update_in(cx, |this, window, cx| {
                if !Arc::ptr_eq(&this.document, &document) {
                    return;
                }
                this.ocr_running = false;
                let message = match failure {
                    Some(error) => error,
                    None if cancelled => format!("OCR stopped ({finished}/{total} pages done)"),
                    None if characters == 0 => "OCR found no text".to_string(),
                    None if total == 1 => {
                        format!("OCR done ({characters} characters). Use Select (V) to copy")
                    }
                    None => format!(
                        "OCR done ({finished} pages, {characters} characters). Use Select (V) to copy"
                    ),
                };
                this.show_notice(&message, window, cx);
            })
            .ok();
        })
        .detach();
    }

    /// ツールバーのOCRボタン。押すと範囲を選ぶメニューが開く。実行中は中断ボタンになる。
    pub(crate) fn render_ocr_button(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let running = self.ocr_running;
        let menu = (self.ocr_menu_open && !running).then(|| self.render_ocr_menu(cx));
        let toggle = if running {
            button("ocr", "Stop", true).on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                this.cancel_ocr(&CancelOcr, window, cx)
            }))
        } else {
            button("ocr", "OCR ▾", self.ocr_menu_open).on_click(cx.listener(
                |this, _: &ClickEvent, _, cx| {
                    this.ocr_menu_open = !this.ocr_menu_open;
                    cx.notify();
                },
            ))
        };
        div().relative().child(toggle).children(menu)
    }

    fn render_ocr_menu(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        // ページの上に重ねて描くため、deferredで他の要素より後に描画する。
        deferred(
            div()
                .absolute()
                .top(px(TOOLBAR_HEIGHT - 6.0))
                .left_0()
                .w(px(MENU_WIDTH))
                .py(px(4.0))
                .rounded_md()
                .border_1()
                .border_color(rgb(MENU_BORDER))
                .bg(rgb(MENU_BACKGROUND))
                .occlude()
                // メニューの外を押したら閉じる。
                .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                    this.ocr_menu_open = false;
                    cx.notify();
                }))
                .child(
                    menu_item("ocr-this-page", "This page").on_click(cx.listener(
                        |this, _: &ClickEvent, window, cx| this.run_ocr(&RunOcr, window, cx),
                    )),
                )
                .child(
                    menu_item("ocr-all-pages", "All pages").on_click(cx.listener(
                        |this, _: &ClickEvent, window, cx| {
                            this.run_ocr_all_pages(&RunOcrAllPages, window, cx)
                        },
                    )),
                ),
        )
        .with_priority(1)
    }
}

fn menu_item(id: &'static str, label: &'static str) -> Stateful<gpui::Div> {
    div()
        .id(id)
        .px(px(12.0))
        .py(px(5.0))
        .cursor_pointer()
        .hover(|d| d.bg(rgb(MENU_HOVER)))
        .child(label)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use gpui::TestAppContext;
    use pdf_core::PdfDocument;

    use crate::{
        actions::{CancelOcr, RunOcr, RunOcrAllPages},
        view::PdfView,
    };

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

    fn open_view(cx: &mut TestAppContext) -> (gpui::Entity<PdfView>, &mut gpui::VisualTestContext) {
        let document = Arc::new(PdfDocument::open(FIXTURE).unwrap());
        cx.add_window_view(|_, cx| PdfView::new(document, cx))
    }

    /// OCRが終わる(Tesseractが無くて失敗する場合も含む)まで待つ。
    fn wait_for_ocr(view: &gpui::Entity<PdfView>, cx: &mut gpui::VisualTestContext) {
        cx.executor().allow_parking();
        for _ in 0..500 {
            cx.run_until_parked();
            if !view.read_with(cx, |this, _| this.ocr_running) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        panic!("OCR never finished");
    }

    #[gpui::test]
    fn the_ocr_button_toggles_the_scope_menu(cx: &mut TestAppContext) {
        let (view, cx) = open_view(cx);
        let open = |view: &gpui::Entity<PdfView>, cx: &gpui::VisualTestContext| {
            view.read_with(cx, |this, _| this.ocr_menu_open)
        };
        assert!(!open(&view, cx));
        view.update_in(cx, |this, _, cx| {
            this.ocr_menu_open = true;
            cx.notify();
        });
        // メニューを開いたまま実行を選ぶと、メニューは閉じる。
        view.update_in(cx, |this, window, cx| this.run_ocr(&RunOcr, window, cx));
        assert!(!open(&view, cx));
        wait_for_ocr(&view, cx);
    }

    /// 全ページを実行すると、全ページが処理され(Tesseractがあれば全ページOCR済みになり)、終わる。
    #[gpui::test]
    fn ocr_all_pages_processes_every_page(cx: &mut TestAppContext) {
        let (view, cx) = open_view(cx);
        view.update_in(cx, |this, window, cx| {
            this.run_ocr_all_pages(&RunOcrAllPages, window, cx)
        });
        assert!(view.read_with(cx, |this, _| this.ocr_running));
        wait_for_ocr(&view, cx);

        let notice = view.read_with(cx, |this, _| this.notice.clone()).unwrap();
        let done = view.read_with(cx, |this, _| {
            (0..this.document.page_count()).all(|i| this.document.is_ocr_done(i))
        });
        // 成功したなら全ページ済み。失敗したなら、エラーのお知らせが出ている。
        assert!(
            done || !notice.starts_with("OCR done"),
            "notice says done but pages are not: {notice}"
        );
    }

    /// 開始してすぐ中断すると、1ページも処理せずに止まる。
    #[gpui::test]
    fn cancelling_stops_before_the_next_page(cx: &mut TestAppContext) {
        let (view, cx) = open_view(cx);
        view.update_in(cx, |this, window, cx| {
            this.run_ocr_all_pages(&RunOcrAllPages, window, cx);
            this.cancel_ocr(&CancelOcr, window, cx);
        });
        wait_for_ocr(&view, cx);

        let (done, notice) = view.read_with(cx, |this, _| {
            (
                (0..this.document.page_count()).any(|i| this.document.is_ocr_done(i)),
                this.notice.clone(),
            )
        });
        assert!(!done, "a page was OCRed after cancelling");
        assert!(
            notice.is_some_and(|n| n.starts_with("OCR stopped")),
            "no stop notice"
        );
    }

    /// 実行中に、もう一度始めても、二重には走らない。
    #[gpui::test]
    fn a_second_start_while_running_is_ignored(cx: &mut TestAppContext) {
        let (view, cx) = open_view(cx);
        view.update_in(cx, |this, window, cx| {
            this.run_ocr_all_pages(&RunOcrAllPages, window, cx);
            let first = Arc::clone(&this.ocr_cancel);
            this.run_ocr(&RunOcr, window, cx);
            assert!(Arc::ptr_eq(&first, &this.ocr_cancel), "restarted");
        });
        wait_for_ocr(&view, cx);
    }
}
