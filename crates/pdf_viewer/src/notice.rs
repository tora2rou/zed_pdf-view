//! 画面の下部に、しばらくだけ出す短いお知らせ(リンクを開いたことなど)。

use std::time::Duration;

use gpui::{Context, IntoElement, SharedString, Window, div, prelude::*, px, rgb};

use crate::view::PdfView;

/// お知らせを出しておく時間。
pub const NOTICE_DURATION: Duration = Duration::from_secs(4);
/// お知らせに入れる文字数の上限(長いURLなどは省略する)。
const NOTICE_MAX_CHARS: usize = 90;

/// 長い文字列を、`max`文字(文字数基準。バイト数ではない)に収めて、省略したら末尾に`…`を付ける。
pub fn shorten(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let kept: String = text.chars().take(max.saturating_sub(1)).collect();
    format!("{kept}…")
}

impl PdfView {
    /// お知らせを出す。`NOTICE_DURATION`たつと消える。すでに出ていれば置き換えて、時間を数え直す。
    pub(crate) fn show_notice(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.notice = Some(SharedString::from(shorten(text, NOTICE_MAX_CHARS)));
        // 前のタスクは、置き換えるときに破棄される(取り消される)。
        self.notice_task = Some(cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(NOTICE_DURATION).await;
            this.update(cx, |this, cx| {
                this.notice = None;
                this.notice_task = None;
                cx.notify();
            })
            .ok();
        }));
        cx.notify();
    }

    /// お知らせが出ているときだけ、下部の中央に小さな帯を重ねる。
    pub(crate) fn render_notice(&self) -> Option<impl IntoElement + use<>> {
        let text = self.notice.clone()?;
        Some(
            div()
                .absolute()
                .bottom(px(16.0))
                .left_0()
                .right_0()
                .flex()
                .justify_center()
                .child(
                    div()
                        .px(px(12.0))
                        .py(px(6.0))
                        .rounded_md()
                        .border_1()
                        .border_color(rgb(0x555555))
                        .bg(rgb(0x1e1e1e))
                        .text_color(rgb(0xdddddd))
                        .text_sm()
                        .child(text),
                ),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_text_is_kept() {
        assert_eq!(shorten("https://example.com", 90), "https://example.com");
        assert_eq!(shorten("abc", 3), "abc");
    }

    #[test]
    fn long_text_is_cut_with_an_ellipsis() {
        assert_eq!(shorten("abcdef", 4), "abc…");
    }

    #[test]
    fn cutting_counts_characters_not_bytes() {
        // 日本語(1文字3バイト)の途中で切って、文字化けや panic を起こさない
        assert_eq!(shorten("あいうえお", 3), "あい…");
        assert_eq!(shorten("あいう", 3), "あいう");
    }

    #[test]
    fn zero_limit_does_not_panic() {
        assert_eq!(shorten("abc", 0), "…");
    }
}
