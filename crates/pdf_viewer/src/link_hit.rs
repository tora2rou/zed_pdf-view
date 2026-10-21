//! リンクのクリック判定と、外部URIの安全確認。GPUIに依存しない純粋関数。

use pdf_core::Link;

/// マウスの押下から解放までの移動がこの距離(論理px)以内なら、ドラッグでなくクリックとみなす。
pub const CLICK_SLOP: f32 = 4.0;

/// 押下位置`down`から解放位置`up`までが、クリックとみなせる移動量か。
pub fn is_click(down: (f32, f32), up: (f32, f32)) -> bool {
    (up.0 - down.0).hypot(up.1 - down.1) <= CLICK_SLOP
}

/// ページ内の点`(x, y)`(倍率1.0・回転前の座標)にあるリンク。重なっているときは、
/// PDFで後に書かれた(上に描かれる)ものを返す。
pub fn link_at(links: &[Link], x: f32, y: f32) -> Option<&Link> {
    links.iter().rev().find(|link| link.rect.contains(x, y))
}

/// 外部URIとして開いてよい文字列なら、そのまま返す。
///
/// PDFは信頼できない入力なので、開くのは`http`・`https`・`mailto`だけにする
/// (`file:`でローカルのファイルを開かせたり、`javascript:`などを実行させたりしないため)。
/// 制御文字を含むものも、不正な入力として開かない。
pub fn openable_uri(uri: &str) -> Option<&str> {
    let uri = uri.trim();
    if uri.chars().any(char::is_control) {
        return None;
    }
    let (scheme, rest) = uri.split_once(':')?;
    let allowed = ["http", "https", "mailto"]
        .iter()
        .any(|s| scheme.eq_ignore_ascii_case(s));
    (allowed && !rest.is_empty()).then_some(uri)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdf_core::{LinkRect, LinkTarget};

    fn link(x: f32, y: f32, w: f32, h: f32, page: usize) -> Link {
        Link {
            rect: LinkRect {
                x,
                y,
                width: w,
                height: h,
            },
            target: LinkTarget::Page(page),
        }
    }

    #[test]
    fn small_movement_is_a_click() {
        assert!(is_click((10.0, 10.0), (10.0, 10.0)));
        assert!(is_click((10.0, 10.0), (12.0, 13.0))); // 距離は約3.6
        assert!(is_click((10.0, 10.0), (14.0, 10.0))); // 距離がちょうど4なら、クリック
        assert!(!is_click((10.0, 10.0), (12.0, 14.0))); // 距離は約4.5
        assert!(!is_click((10.0, 10.0), (20.0, 10.0)));
    }

    #[test]
    fn link_at_finds_the_link_under_the_point() {
        let links = [
            link(0.0, 0.0, 10.0, 10.0, 1),
            link(20.0, 0.0, 10.0, 10.0, 2),
        ];
        assert_eq!(
            link_at(&links, 5.0, 5.0).unwrap().target,
            LinkTarget::Page(1)
        );
        assert_eq!(
            link_at(&links, 25.0, 5.0).unwrap().target,
            LinkTarget::Page(2)
        );
        assert!(link_at(&links, 15.0, 5.0).is_none());
        assert!(link_at(&[], 0.0, 0.0).is_none());
    }

    #[test]
    fn topmost_link_wins_when_overlapping() {
        let links = [link(0.0, 0.0, 20.0, 20.0, 1), link(5.0, 5.0, 5.0, 5.0, 2)];
        assert_eq!(
            link_at(&links, 6.0, 6.0).unwrap().target,
            LinkTarget::Page(2)
        );
        assert_eq!(
            link_at(&links, 1.0, 1.0).unwrap().target,
            LinkTarget::Page(1)
        );
    }

    #[test]
    fn web_and_mail_uris_are_openable() {
        assert_eq!(
            openable_uri("https://example.com/a?b=c"),
            Some("https://example.com/a?b=c")
        );
        assert_eq!(
            openable_uri("http://example.org"),
            Some("http://example.org")
        );
        assert_eq!(
            openable_uri("mailto:me@example.com"),
            Some("mailto:me@example.com")
        );
        assert_eq!(
            openable_uri("  HTTPS://Example.com "),
            Some("HTTPS://Example.com")
        );
    }

    #[test]
    fn dangerous_or_unknown_schemes_are_refused() {
        for uri in [
            "javascript:alert(1)",
            "file:///etc/passwd",
            "ftp://example.com",
            "data:text/html,<script>",
            "vbscript:x",
            "ssh://host",
            "//example.com",
            "example.com",
            "",
            ":",
            "https:",
        ] {
            assert_eq!(openable_uri(uri), None, "{uri}");
        }
    }

    #[test]
    fn control_characters_are_refused() {
        assert_eq!(openable_uri("https://example.com/\nfoo"), None);
        assert_eq!(openable_uri("https://exa\0mple.com"), None);
    }
}
