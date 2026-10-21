//! ページの文字から、URLらしい部分を見つけてリンクにする(注釈が無くても、ブラウザのように踏めるように)。

use crate::{
    links::{Link, LinkRect, LinkTarget},
    text::{Gap, PageText, TextChar},
};

/// URLの前につく文字列(小文字で比べる)。`www.`で始まるものは、`https://`を補って開く。
const SCHEMES: [&str; 3] = ["https://", "http://", "mailto:"];
/// URLの長さの上限(文字数)。
const MAX_URL_CHARS: usize = 2048;

/// URLに使える文字。非ASCII(日本語など)や空白は含まない。URLの直後に続く日本語を巻き込まないため。
fn is_url_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || "-._~:/?#[]@!$&'()*+,;=%".contains(c)
}

/// URLの末尾から取り除く句読点。文中でURLのすぐ後ろに付きやすい。
fn is_trailing_punctuation(c: char) -> bool {
    ".,;:!?'\"".contains(c)
}

/// 文字の並びのうち、URLとして使う範囲を決める(`candidate`はURL候補の文字列)。
/// 先頭から、URLに使えない文字が出るまでを取り、末尾の句読点と、対応する開き括弧が無い閉じ括弧を外す。
fn url_prefix(candidate: &str) -> &str {
    let end = candidate
        .char_indices()
        .find(|&(_, c)| !is_url_char(c))
        .map_or(candidate.len(), |(i, _)| i);
    let mut url = &candidate[..end];
    while let Some(last) = url.chars().next_back() {
        let unbalanced_close = |open: char, close: char| {
            last == close && url.matches(close).count() > url.matches(open).count()
        };
        if is_trailing_punctuation(last) || unbalanced_close('(', ')') || unbalanced_close('[', ']')
        {
            url = &url[..url.len() - last.len_utf8()];
        } else {
            break;
        }
    }
    url
}

/// 語(空白と区切りで切った文字の連なり)の中から、URLを1つ探す。
/// 戻り値は、語の中での(開始位置, 文字数, URLの文字列)。
fn find_url_in_word(word: &str) -> Option<(usize, usize, String)> {
    let lower = word.to_ascii_lowercase();
    // 一番早く現れるスキームを探す(語の先頭に括弧などが付いていてもよい)
    let found = SCHEMES
        .iter()
        .filter_map(|scheme| lower.find(scheme).map(|i| (i, *scheme)))
        .chain(lower.find("www.").map(|i| (i, "www.")))
        .min_by_key(|&(i, _)| i)?;
    let (start_byte, scheme) = found;
    let url = url_prefix(&word[start_byte..]);
    // スキームの後ろに、ホスト名として使える文字が1つ以上あること
    let rest = &url[scheme.len().min(url.len())..];
    if !rest
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphanumeric())
    {
        return None;
    }
    // ホストにドットが無いhttp(s)は、普通のURLではない(例: "https://x")ので除く
    let is_web = scheme != "mailto:";
    if is_web
        && !rest
            .split(['/', '?', '#'])
            .next()
            .is_some_and(|host| host.contains('.'))
    {
        return None;
    }
    let chars = url.chars().count();
    let start = word[..start_byte].chars().count();
    let target = if scheme == "www." {
        format!("https://{url}")
    } else {
        url.to_string()
    };
    Some((start, chars, target))
}

/// ページの文字からURLを見つけ、それぞれの範囲を`Link`にして返す(描画順)。
/// 1つのURLは、同じ語(区切りも空白もなく続く文字)の中だけで探す。行をまたぐURLは先頭の行の分だけになる。
pub(crate) fn detect_urls(text: &PageText) -> Vec<Link> {
    let mut links = Vec::new();
    let mut word: Vec<&TextChar> = Vec::new();
    for c in &text.chars {
        let is_space = c.text.chars().all(char::is_whitespace);
        // 空白の文字(PDFによっては、空白も文字として描かれる)と、位置の区切りが、語の切れ目
        if is_space || c.gap != Gap::None {
            links.extend(url_in_word(&word));
            word.clear();
        }
        if !is_space {
            word.push(c);
        }
    }
    links.extend(url_in_word(&word));
    links
}

/// 1つの語(`chars`)にURLがあれば、その範囲の`Link`にする。
fn url_in_word(chars: &[&TextChar]) -> Option<Link> {
    // 合字など、1つの文字が複数文字のものを含む語は、文字数の対応が取れないので対象にしない。
    if chars.is_empty() || chars.iter().any(|c| c.text.chars().count() != 1) {
        return None;
    }
    let word: String = chars.iter().map(|c| c.text.as_str()).collect();
    let (offset, count, target) = find_url_in_word(&word)?;
    if count > MAX_URL_CHARS {
        return None;
    }
    let rect = union(chars[offset..offset + count].iter().map(|c| c.rect))?;
    Some(Link {
        rect,
        target: LinkTarget::Uri(target),
    })
}

/// 矩形の集まりを覆う、最小の矩形。
fn union(rects: impl Iterator<Item = LinkRect>) -> Option<LinkRect> {
    rects.reduce(|a, b| {
        let (x0, y0) = (a.x.min(b.x), a.y.min(b.y));
        let (x1, y1) = (
            (a.x + a.width).max(b.x + b.width),
            (a.y + a.height).max(b.y + b.height),
        );
        LinkRect {
            x: x0,
            y: y0,
            width: x1 - x0,
            height: y1 - y0,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::TextChar;

    fn url(word: &str) -> Option<String> {
        find_url_in_word(word).map(|(_, _, u)| u)
    }

    #[test]
    fn finds_plain_urls() {
        assert_eq!(
            url("https://example.com/a/doc.pdf").as_deref(),
            Some("https://example.com/a/doc.pdf")
        );
        assert_eq!(
            url("http://example.org/x?y=1").as_deref(),
            Some("http://example.org/x?y=1")
        );
        assert_eq!(
            url("HTTPS://Example.com").as_deref(),
            Some("HTTPS://Example.com")
        );
    }

    #[test]
    fn strips_trailing_punctuation() {
        assert_eq!(
            url("https://example.com/a.").as_deref(),
            Some("https://example.com/a")
        );
        assert_eq!(
            url("https://example.com/a,").as_deref(),
            Some("https://example.com/a")
        );
        assert_eq!(
            url("https://example.com/a.),").as_deref(),
            Some("https://example.com/a")
        );
        assert_eq!(
            url("https://example.com/a?").as_deref(),
            Some("https://example.com/a")
        );
    }

    #[test]
    fn handles_surrounding_and_balanced_parentheses() {
        // 文中の括弧で囲まれたURLは、閉じ括弧を含めない
        assert_eq!(
            url("(http://example.org/x?y=1),").as_deref(),
            Some("http://example.org/x?y=1")
        );
        // URL自身が括弧を含むときは、対応していれば残す
        assert_eq!(
            url("https://en.wikipedia.org/wiki/Foo_(bar)").as_deref(),
            Some("https://en.wikipedia.org/wiki/Foo_(bar)")
        );
        assert_eq!(
            url("https://example.com/[x]").as_deref(),
            Some("https://example.com/[x]")
        );
    }

    #[test]
    fn stops_at_non_ascii_characters() {
        // 日本語がすぐ後ろに続いても、巻き込まない
        assert_eq!(
            url("https://www.mhlw.go.jp/a.html、次").as_deref(),
            Some("https://www.mhlw.go.jp/a.html")
        );
        assert_eq!(
            url("参照https://example.com/").as_deref(),
            Some("https://example.com/")
        );
    }

    #[test]
    fn www_gets_a_scheme() {
        assert_eq!(
            url("www.example.net.").as_deref(),
            Some("https://www.example.net")
        );
    }

    #[test]
    fn mailto_is_supported() {
        assert_eq!(
            url("mailto:me@example.com").as_deref(),
            Some("mailto:me@example.com")
        );
    }

    #[test]
    fn rejects_things_that_are_not_urls() {
        assert_eq!(url("https://"), None);
        assert_eq!(url("https://."), None);
        assert_eq!(url("http://x"), None); // ホストにドットが無い
        assert_eq!(url("example"), None);
        assert_eq!(url("ftp://example.com"), None);
        assert_eq!(url("mailto:"), None);
        assert_eq!(url(""), None);
    }

    fn chars(words: &[(&str, Gap)]) -> PageText {
        // 各文字を、幅10の箱として横に並べる
        let mut x = 0.0;
        let mut out = Vec::new();
        for (text, first_gap) in words {
            for (i, c) in text.chars().enumerate() {
                out.push(TextChar {
                    text: c.to_string(),
                    rect: LinkRect {
                        x,
                        y: 0.0,
                        width: 10.0,
                        height: 12.0,
                    },
                    gap: if i == 0 { *first_gap } else { Gap::None },
                    dir: (1.0, 0.0),
                });
                x += 10.0;
            }
            x += 10.0;
        }
        PageText { chars: out }
    }

    #[test]
    fn detects_a_url_and_covers_exactly_its_characters() {
        // "see" + 空白 + "(https://a.jp)." : 括弧とピリオドは含めない
        let text = chars(&[("see", Gap::Line), ("(https://a.jp).", Gap::Space)]);
        let links = detect_urls(&text);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target, LinkTarget::Uri("https://a.jp".to_string()));
        // 開始: "see"(3文字)+間(1文字分)+"("(1文字) = 5文字目から。幅は "https://a.jp" の12文字
        let r = links[0].rect;
        assert_eq!(r.x, 50.0);
        assert_eq!(r.width, 120.0);
        assert_eq!(r.height, 12.0);
    }

    #[test]
    fn words_split_by_gaps_are_separate() {
        let text = chars(&[
            ("https://a.jp/x", Gap::Line),
            ("https://b.jp/y", Gap::Space),
            ("no", Gap::Line),
        ]);
        let targets: Vec<_> = detect_urls(&text).into_iter().map(|l| l.target).collect();
        assert_eq!(
            targets,
            [
                LinkTarget::Uri("https://a.jp/x".to_string()),
                LinkTarget::Uri("https://b.jp/y".to_string())
            ]
        );
    }

    /// 空白が1文字として描かれているPDFでも、その直後のURLを見つける(空白の文字に巻き込まれて飛ばさない)。
    #[test]
    fn a_url_after_an_explicit_space_glyph_is_found() {
        let mut text = chars(&[
            ("see", Gap::Line),
            (" ", Gap::None),
            ("https://a.jp/x", Gap::None),
        ]);
        // 空白の文字の次は、隣り合っている(区切りなし)ものとして並べ直す
        for (i, c) in text.chars.iter_mut().enumerate() {
            c.gap = if i == 0 { Gap::Line } else { Gap::None };
        }
        let targets: Vec<_> = detect_urls(&text).into_iter().map(|l| l.target).collect();
        assert_eq!(targets, [LinkTarget::Uri("https://a.jp/x".to_string())]);
    }

    #[test]
    fn empty_page_has_no_urls() {
        assert!(detect_urls(&PageText::default()).is_empty());
    }
}
