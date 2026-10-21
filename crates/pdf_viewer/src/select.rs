//! テキスト選択(ドラッグ・ダブルクリック・コピー・全選択)。

use std::sync::Arc;

use gpui::{ClipboardItem, Context, MouseDownEvent, Pixels, Point, Window, rgba};
use pdf_core::{LinkRect, PdfDocument};

use crate::{
    actions::{ClearSelection, CopySelection, HandTool, SelectAll, SelectTool},
    pan::edge_autoscroll,
    selection::{Caret, Selection},
    view::{PdfView, Tool},
};

/// 選択範囲のハイライトの色。
pub const SELECTION_COLOR: u32 = 0x4d9bff66;

/// 選択範囲の文字列。複数のページにまたがるときは、ページごとの文字列を改行でつなぐ。
/// 文字を取り出していないページの分は、その場で取り出す(重いので、バックグラウンドで呼ぶ)。
pub fn selected_text(document: &PdfDocument, selection: &Selection) -> String {
    let parts: Vec<String> = selection
        .pages()
        .filter_map(|page| {
            let text = document.page_text(page).ok()?;
            let range = selection.range_on_page(page, text.chars.len())?;
            Some(text.string_in(range))
        })
        .filter(|part| !part.is_empty())
        .collect();
    parts.join("\n")
}

impl PdfView {
    /// ウィンドウ上の位置`position`に最も近い、文字と文字の間(キャレット)と、その位置の最寄りの文字の番号。
    /// ページの外(余白や隙間)でも、最も近いページの縁に丸めて求める(ドラッグを続けられるように)。
    /// そのページの文字がまだ取り出されていないとき、文字の無いページのときは`None`。
    fn caret_at_position(
        &self,
        position: Point<Pixels>,
        window: &Window,
    ) -> Option<(Caret, usize)> {
        let bounds = self.scroll_handle.bounds();
        let (viewport_width, _) = self.viewport_size(window);
        let (scroll_x, scroll_y) = self.scroll_position();
        let x = scroll_x + f32::from(position.x - bounds.origin.x);
        let y = scroll_y + f32::from(position.y - bounds.origin.y);

        let (page, local_x, local_y) = self.layout(viewport_width).nearest_page_point(x, y)?;
        let text = self.document.page_text_if_ready(page)?;
        // 画面上のpx → 回転後の画像上のポイント → 回転前の画像上のポイント(文字の座標系)
        let shown = (local_x / self.zoom, local_y / self.zoom);
        let base = *self.base_sizes.get(page)?;
        let (bx, by) = self.appearance.rotation.to_base(base, shown);
        let index = text.caret_at(bx, by)?;
        let char_index = text.nearest_char(bx, by)?;
        Some((Caret { page, index }, char_index))
    }

    /// 選択ツールでマウスを押した。1回なら選択の開始(ドラッグで広げる)、2回なら語、3回以上なら行を選ぶ。
    pub(crate) fn begin_selection(
        &mut self,
        event: &MouseDownEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        self.last_pointer = Some(event.position);
        let Some((caret, char_index)) = self.caret_at_position(event.position, window) else {
            self.selection = None;
            self.selecting = false;
            cx.notify();
            return;
        };
        let range_of = |page: usize, f: &dyn Fn(&pdf_core::PageText) -> std::ops::Range<usize>| {
            self.document.page_text_if_ready(page).map(f)
        };
        match event.click_count {
            2 => {
                self.selecting = false;
                self.selection =
                    range_of(caret.page, &|t| t.word_range(char_index)).map(|r| Selection {
                        anchor: Caret {
                            page: caret.page,
                            index: r.start,
                        },
                        focus: Caret {
                            page: caret.page,
                            index: r.end,
                        },
                    });
            }
            n if n >= 3 => {
                self.selecting = false;
                self.selection =
                    range_of(caret.page, &|t| t.line_range(char_index)).map(|r| Selection {
                        anchor: Caret {
                            page: caret.page,
                            index: r.start,
                        },
                        focus: Caret {
                            page: caret.page,
                            index: r.end,
                        },
                    });
            }
            _ => {
                self.selecting = true;
                self.selection = Some(Selection {
                    anchor: caret,
                    focus: caret,
                });
            }
        }
        cx.notify();
    }

    /// ドラッグ中に、選択の動かす側の端を、ポインタの位置に合わせる。
    pub(crate) fn extend_selection(
        &mut self,
        position: Point<Pixels>,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        self.last_pointer = Some(position);
        if !self.selecting {
            return;
        }
        self.follow_pointer(window, cx);
    }

    /// 最後に分かっているポインタの位置に、選択の端を合わせる(スクロールで内容が動いたときにも使う)。
    fn follow_pointer(&mut self, window: &Window, cx: &mut Context<Self>) {
        let Some(position) = self.last_pointer else {
            return;
        };
        let Some((caret, _)) = self.caret_at_position(position, window) else {
            return;
        };
        if let Some(selection) = self.selection.as_mut()
            && selection.focus != caret
        {
            selection.focus = caret;
            cx.notify();
        }
    }

    /// ドラッグ中、ポインタがページ領域の上下に出ていたら自動でスクロールして、選択の端を追従させる。
    /// 描画のたびに呼ばれる(続けて再描画することで、スクロールが滑らかに続く)。
    pub(crate) fn autoscroll_selection(&mut self, window: &Window, cx: &mut Context<Self>) {
        if !self.selecting {
            return;
        }
        if let Some(position) = self.last_pointer {
            let bounds = self.scroll_handle.bounds();
            let delta = edge_autoscroll(
                f32::from(position.y),
                f32::from(bounds.origin.y),
                f32::from(bounds.origin.y + bounds.size.height),
            );
            if delta != 0.0 {
                let (x, y) = self.scroll_position();
                self.set_scroll(x, (y + delta).max(0.0), cx);
            }
        }
        self.follow_pointer(window, cx);
    }

    /// ボタンを離した。クリック(ほぼ動かさずに離した)なら、範囲の無い選択は解除する。
    pub(crate) fn finish_selection(&mut self, cx: &mut Context<Self>) {
        self.selecting = false;
        if self.selection.is_some_and(|s| s.is_empty()) {
            self.selection = None;
        }
        cx.notify();
    }

    pub(crate) fn set_tool(&mut self, tool: Tool, cx: &mut Context<Self>) {
        if self.tool != tool {
            self.tool = tool;
            self.drag = None;
            self.selecting = false;
            cx.notify();
        }
    }

    pub(crate) fn hand_tool(&mut self, _: &HandTool, _: &mut Window, cx: &mut Context<Self>) {
        self.set_tool(Tool::Hand, cx);
    }

    pub(crate) fn select_tool(&mut self, _: &SelectTool, _: &mut Window, cx: &mut Context<Self>) {
        self.set_tool(Tool::Select, cx);
    }

    pub(crate) fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.selection = Selection::all(self.base_sizes.len());
        self.selecting = false;
        cx.notify();
    }

    pub(crate) fn clear_selection(
        &mut self,
        _: &ClearSelection,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selection.take().is_some() {
            self.selecting = false;
            cx.notify();
        }
    }

    /// 選択範囲の文字列をクリップボードにコピーする。ページの文字の取り出しが重いことがあるので、
    /// バックグラウンドで集める。
    pub(crate) fn copy_selection(
        &mut self,
        _: &CopySelection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(selection) = self.selection.filter(|s| !s.is_empty()) else {
            return;
        };
        let document: Arc<PdfDocument> = self.document.clone();
        cx.spawn_in(window, async move |this, cx| {
            let text = gpui::AppContext::background_spawn(&*cx, async move {
                selected_text(&document, &selection)
            })
            .await;
            this.update_in(cx, |this, window, cx| {
                if text.is_empty() {
                    return;
                }
                let count = text.chars().count();
                cx.write_to_clipboard(ClipboardItem::new_string(text));
                this.show_notice(&format!("Copied {count} characters"), window, cx);
            })
            .ok();
        })
        .detach();
    }

    /// ページ`page`の選択範囲のハイライト(回転後の画像上の矩形、ポイント単位)。
    pub(crate) fn selection_highlights(&self, page: usize) -> Vec<LinkRect> {
        let Some(selection) = self.selection else {
            return Vec::new();
        };
        let (Some(text), Some(&base)) = (
            self.document.page_text_if_ready(page),
            self.base_sizes.get(page),
        ) else {
            return Vec::new();
        };
        let Some(range) = selection.range_on_page(page, text.chars.len()) else {
            return Vec::new();
        };
        text.selection_rects(range)
            .into_iter()
            .map(|rect| self.appearance.rotation.rotate_rect(base, rect))
            .collect()
    }
}

/// ハイライトの色。
pub fn highlight_color() -> gpui::Rgba {
    rgba(SELECTION_COLOR)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{actions::ToggleSidebar, view::Tool};
    use gpui::{
        Modifiers, MouseButton, MouseDownEvent, TestAppContext, VisualTestContext, point, px,
    };

    const WITH_TEXT: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../pdf_core/tests/fixtures/with_text.pdf"
    );

    /// 選択ツールで開いたビュー。文字の取り出し(バックグラウンド)が済むまで進める。
    fn open_selecting(cx: &mut TestAppContext) -> (gpui::Entity<PdfView>, &mut VisualTestContext) {
        cx.update(|cx| crate::init(cx));
        let document = Arc::new(PdfDocument::open(WITH_TEXT).unwrap());
        let (view, cx) = cx.add_window_view(|_, cx| PdfView::new(document, cx));
        view.update_in(cx, |this, _, cx| this.set_tool(Tool::Select, cx));
        cx.run_until_parked();
        (view, cx)
    }

    /// ページ`page`の`index`番目の文字の、手前の縁(`edge` = -1)・中心(0)・向こうの縁(+1)の、ウィンドウ上の位置。
    fn char_point(
        view: &gpui::Entity<PdfView>,
        cx: &mut VisualTestContext,
        page: usize,
        index: usize,
        edge: f32,
    ) -> gpui::Point<gpui::Pixels> {
        view.update_in(cx, |this, window, _| {
            let text = this.document.page_text(page).unwrap();
            let c = &text.chars[index];
            // 文字の中心から、進む向きに半分ずれた(少し内側の)位置
            let (cx0, cy0) = (
                c.rect.x + c.rect.width / 2.0,
                c.rect.y + c.rect.height / 2.0,
            );
            let half = if c.dir.0.abs() > c.dir.1.abs() {
                c.rect.width
            } else {
                c.rect.height
            } / 2.0;
            let (bx, by) = (
                cx0 + c.dir.0 * half * 0.6 * edge,
                cy0 + c.dir.1 * half * 0.6 * edge,
            );
            let base = this.base_sizes[page];
            let shown = this.appearance.rotation.point_from_base(base, (bx, by));
            let (vw, _) = this.viewport_size(window);
            let page_rect = this.layout(vw).pages[page];
            let (sx, sy) = this.scroll_position();
            let bounds = this.scroll_handle.bounds();
            point(
                bounds.origin.x + px(page_rect.left + shown.0 * this.zoom - sx),
                bounds.origin.y + px(page_rect.top + shown.1 * this.zoom - sy),
            )
        })
    }

    fn drag(
        cx: &mut VisualTestContext,
        from: gpui::Point<gpui::Pixels>,
        to: gpui::Point<gpui::Pixels>,
    ) {
        cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
        cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::none());
        cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
    }

    /// 2つの文字の位置(ページ, 番号, 縁)の間をドラッグする。
    fn drag_between(
        view: &gpui::Entity<PdfView>,
        cx: &mut VisualTestContext,
        from: (usize, usize, f32),
        to: (usize, usize, f32),
    ) {
        let from = char_point(view, cx, from.0, from.1, from.2);
        let to = char_point(view, cx, to.0, to.1, to.2);
        drag(cx, from, to);
    }

    fn clipboard(cx: &mut VisualTestContext) -> Option<String> {
        cx.read_from_clipboard().and_then(|item| item.text())
    }

    /// 文字をドラッグで選んで、Ctrl+Cでコピーできる。
    #[gpui::test]
    fn dragging_selects_text_and_ctrl_c_copies_it(cx: &mut TestAppContext) {
        let (view, cx) = open_selecting(cx);
        // 1行目 "See https://example.com/a/doc.pdf for details." の "https://example.com" を選ぶ
        // 'h' は4番目(添字4)、"https://example.com" は19文字なので、最後の 'm' は添字22
        let from = char_point(&view, cx, 0, 4, -1.0);
        let to = char_point(&view, cx, 0, 22, 1.0);
        drag(cx, from, to);

        assert!(view.read_with(cx, |this, _| this.selection.is_some_and(|s| !s.is_empty())));
        cx.simulate_keystrokes("ctrl-c");
        cx.run_until_parked();
        assert_eq!(clipboard(cx).as_deref(), Some("https://example.com"));
        let notice = view.read_with(cx, |this, _| this.notice.clone());
        assert_eq!(notice.as_deref(), Some("Copied 19 characters"));
    }

    /// 逆向き(右から左)にドラッグしても、同じ範囲が選ばれる。
    #[gpui::test]
    fn dragging_backwards_selects_the_same_text(cx: &mut TestAppContext) {
        let (view, cx) = open_selecting(cx);
        let from = char_point(&view, cx, 0, 22, 1.0);
        let to = char_point(&view, cx, 0, 4, -1.0);
        drag(cx, from, to);
        cx.simulate_keystrokes("ctrl-c");
        cx.run_until_parked();
        assert_eq!(clipboard(cx).as_deref(), Some("https://example.com"));
    }

    /// ハイライトは、選んだ文字の上に出る。
    #[gpui::test]
    fn selected_text_is_highlighted(cx: &mut TestAppContext) {
        let (view, cx) = open_selecting(cx);
        assert!(view.read_with(cx, |this, _| this.selection_highlights(0).is_empty()));
        drag_between(&view, cx, (0, 4, -1.0), (0, 22, 1.0));
        let rects = view.read_with(cx, |this, _| this.selection_highlights(0));
        assert_eq!(rects.len(), 1, "one line = one rectangle");
        // 1行目(表示のy=30付近)の上で、"https://example.com" の19文字ぶん(約110pt)の幅
        let r = rects[0];
        assert!(r.y < 30.0 && r.y + r.height > 30.0, "{r:?}");
        assert!(r.width > 90.0 && r.width < 130.0, "{r:?}");
    }

    /// ダブルクリックで語、トリプルクリックで行を選ぶ。
    #[gpui::test]
    fn double_click_selects_a_word_and_triple_click_a_line(cx: &mut TestAppContext) {
        let (view, cx) = open_selecting(cx);
        // "details" の中(1行目の 'd' は添字39前後。"details." の 'e' を狙う)
        let text_index = view.update_in(cx, |this, _, _| {
            let t = this.document.page_text(0).unwrap().to_plain_string();
            t.find("details").unwrap() + 1
        });
        let at = char_point(&view, cx, 0, text_index, 0.0);
        let click = |count: usize, cx: &mut VisualTestContext| {
            cx.simulate_event(MouseDownEvent {
                position: at,
                modifiers: Modifiers::none(),
                button: MouseButton::Left,
                click_count: count,
                first_mouse: false,
            });
            cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
        };
        click(2, cx);
        cx.simulate_keystrokes("ctrl-c");
        cx.run_until_parked();
        assert_eq!(clipboard(cx).as_deref(), Some("details"));

        click(3, cx);
        cx.simulate_keystrokes("ctrl-c");
        cx.run_until_parked();
        assert_eq!(
            clipboard(cx).as_deref(),
            Some("See https://example.com/a/doc.pdf for details.")
        );
    }

    /// 複数のページにまたがってドラッグすると、ページごとの文字列が改行でつながる。
    #[gpui::test]
    fn selection_can_span_pages(cx: &mut TestAppContext) {
        let (view, cx) = open_selecting(cx);
        // 1ページ目の最後の行 "Mail: ..." の 'M' から、2ページ目(回転)の "Rotated" の7文字目まで
        let m = view.update_in(cx, |this, _, _| {
            let text = this.document.page_text(0).unwrap();
            // 最後の行("Mail: ...")の先頭の文字
            text.chars
                .iter()
                .rposition(|c| c.gap == pdf_core::Gap::Line)
                .unwrap()
        });
        let from = char_point(&view, cx, 0, m, -1.0);
        let to = char_point(&view, cx, 1, 6, 1.0);
        drag(cx, from, to);
        cx.simulate_keystrokes("ctrl-c");
        cx.run_until_parked();
        assert_eq!(
            clipboard(cx).as_deref(),
            Some("Mail: mailto:me@example.com ok\nRotated")
        );
    }

    /// 手のひらツールでは、ドラッグしても文字は選ばれない。
    #[gpui::test]
    fn the_hand_tool_does_not_select(cx: &mut TestAppContext) {
        let (view, cx) = open_selecting(cx);
        view.update_in(cx, |this, _, cx| this.set_tool(Tool::Hand, cx));
        drag_between(&view, cx, (0, 4, -1.0), (0, 22, 1.0));
        assert!(view.read_with(cx, |this, _| this.selection.is_none()));
    }

    /// クリックだけでは範囲が無いので、選択は解除される。Escでも解除できる。
    #[gpui::test]
    fn click_and_escape_clear_the_selection(cx: &mut TestAppContext) {
        let (view, cx) = open_selecting(cx);
        drag_between(&view, cx, (0, 4, -1.0), (0, 22, 1.0));
        assert!(view.read_with(cx, |this, _| this.selection.is_some()));
        cx.simulate_keystrokes("escape");
        assert!(view.read_with(cx, |this, _| this.selection.is_none()));

        drag_between(&view, cx, (0, 4, -1.0), (0, 22, 1.0));
        let at = char_point(&view, cx, 0, 10, 0.0);
        cx.simulate_click(at, Modifiers::none());
        assert!(view.read_with(cx, |this, _| this.selection.is_none()));
    }

    /// Ctrl+Aで全部を選んでコピーすると、全ページの文字がつながる。
    #[gpui::test]
    fn select_all_copies_the_whole_document(cx: &mut TestAppContext) {
        let (view, cx) = open_selecting(cx);
        // キー操作は、フォーカスのある要素に届く(アプリでは起動時にフォーカスを渡している)
        view.update_in(cx, |this, window, cx| window.focus(&this.focus_handle, cx));
        cx.simulate_keystrokes("ctrl-a");
        cx.simulate_keystrokes("ctrl-c");
        cx.run_until_parked();
        let copied = clipboard(cx).unwrap();
        assert!(copied.starts_with("See https://example.com/a/doc.pdf for details.\n"));
        assert!(copied.ends_with("\nRotated https://example.com/r.pdf here"));
        let _ = view;
    }

    /// 回転していても、ドラッグした範囲の文字が選ばれ、ハイライトは回転後の向きになる。
    #[gpui::test]
    fn selecting_works_on_a_rotated_view(cx: &mut TestAppContext) {
        let (view, cx) = open_selecting(cx);
        view.update_in(cx, |this, window, cx| {
            this.rotate_clockwise(&crate::actions::RotateClockwise, window, cx)
        });
        cx.run_until_parked();
        drag_between(&view, cx, (0, 4, -1.0), (0, 22, 1.0));
        cx.simulate_keystrokes("ctrl-c");
        cx.run_until_parked();
        assert_eq!(clipboard(cx).as_deref(), Some("https://example.com"));
        // 90度回転すると、横に並んだ文字のハイライトは縦長になる
        let rects = view.read_with(cx, |this, _| this.selection_highlights(0));
        assert!(rects[0].height > rects[0].width * 3.0, "{:?}", rects[0]);
    }

    #[gpui::test]
    fn selection_is_dropped_when_the_document_changes(cx: &mut TestAppContext) {
        let (view, cx) = open_selecting(cx);
        drag_between(&view, cx, (0, 4, -1.0), (0, 22, 1.0));
        let other = Arc::new(PdfDocument::open(WITH_TEXT).unwrap());
        view.update_in(cx, |this, window, cx| this.set_document(other, window, cx));
        assert!(view.read_with(cx, |this, _| this.selection.is_none()));
        let _ = ToggleSidebar;
    }
}
