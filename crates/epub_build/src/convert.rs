//! 章のXHTMLを、Typstのマークアップに変換する。

use std::collections::{HashMap, HashSet};

use scraper::{ElementRef, Html, Node, Selector};

use crate::package::{percent_decode, resolve};

/// これより深い入れ子は、変換しない(壊れた/悪意あるEPUBでスタックが尽きないための安全弁)。
const MAX_DEPTH: usize = 120;

/// 画像として扱う拡張子(Typstが読めるもの)。
const IMAGE_EXTENSIONS: [&str; 6] = ["png", "jpg", "jpeg", "gif", "svg", "webp"];

/// 章1つぶん(ZIPの中のパスと、XHTMLの文字列)。
pub struct Chapter {
    pub path: String,
    pub html: String,
}

/// 変換の結果。
#[derive(Debug, Default)]
pub struct Converted {
    /// 本文のTypstマークアップ(前置き `preamble` は含まない)。
    pub body: String,
    /// 文書で使う画像: (ZIPの中のパス, 出力先の相対パス)。
    pub resources: Vec<(String, String)>,
    /// 変換できなかったものなどの知らせ。
    pub warnings: Vec<String>,
}

/// 文字列を、Typstの文字列リテラルの中身にする。
pub fn string_literal(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' | '\t' => out.push(' '),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

/// 日本語(かな・漢字・全角・記号)の文字か。
fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x3000..=0x30FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF | 0xFF00..=0xFFEF)
}

/// HTMLの空白(連続する空白・改行)を1つの空白にまとめる。日本語どうしの間の改行は、取り除く
/// (ソースの途中で折り返した改行が、空白として見えないように)。
pub fn collapse_whitespace(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if !matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{c}') {
            out.push(c);
            continue;
        }
        let mut newline = c == '\n';
        while let Some(&next) = chars.peek() {
            if !matches!(next, ' ' | '\t' | '\n' | '\r' | '\u{c}') {
                break;
            }
            newline |= next == '\n';
            chars.next();
        }
        let before = out.chars().last();
        let after = chars.peek().copied();
        let between_cjk = before.is_some_and(is_cjk) && after.is_some_and(is_cjk);
        if !(newline && between_cjk) {
            out.push(' ');
        }
    }
    out
}

/// 文書の断片。文字は`Text`(まとめてリテラルになる)、それ以外は`Code`(Typstのコードそのまま)。
enum Piece {
    Text(String),
    Code(String),
}

#[derive(Default)]
struct Inline {
    pieces: Vec<Piece>,
}

impl Inline {
    fn text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        // 空白が2つ続かないようにする(要素をまたいでも)。
        let text = match self.pieces.last() {
            Some(Piece::Text(prev)) if prev.ends_with(' ') => text.trim_start_matches(' '),
            _ => text,
        };
        if text.is_empty() {
            return;
        }
        match self.pieces.last_mut() {
            Some(Piece::Text(prev)) => prev.push_str(text),
            _ => self.pieces.push(Piece::Text(text.to_string())),
        }
    }

    fn code(&mut self, code: impl Into<String>) {
        self.pieces.push(Piece::Code(code.into()));
    }

    /// 先頭・末尾の空白を落とす。
    fn trim(&mut self) {
        if let Some(Piece::Text(first)) = self.pieces.first_mut() {
            *first = first.trim_start_matches(' ').to_string();
        }
        if let Some(Piece::Text(last)) = self.pieces.last_mut() {
            *last = last.trim_end_matches(' ').to_string();
        }
        self.pieces
            .retain(|p| !matches!(p, Piece::Text(t) if t.is_empty()));
    }

    /// 文字も画像もなく、アンカーなどの目に見えないものだけか。
    fn is_blank(&self) -> bool {
        self.pieces.iter().all(|p| match p {
            Piece::Text(t) => t.trim().is_empty(),
            Piece::Code(c) => c.starts_with("#box(width: 0pt"),
        })
    }

    fn render(&self) -> String {
        let mut out = String::new();
        for piece in &self.pieces {
            match piece {
                Piece::Text(t) => {
                    out.push_str("#\"");
                    out.push_str(&string_literal(t));
                    out.push('"');
                }
                Piece::Code(c) => out.push_str(c),
            }
        }
        out
    }
}

/// ブロックとして扱うタグ。
fn is_block(name: &str) -> bool {
    matches!(
        name,
        "p" | "div"
            | "section"
            | "article"
            | "aside"
            | "header"
            | "footer"
            | "main"
            | "nav"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "ul"
            | "ol"
            | "li"
            | "dl"
            | "dt"
            | "dd"
            | "blockquote"
            | "pre"
            | "hr"
            | "table"
            | "figure"
            | "figcaption"
            | "address"
            | "body"
            | "center"
            | "hgroup"
            | "details"
            | "summary"
    )
}

/// 中身を読まないタグ。
fn is_skipped(name: &str) -> bool {
    matches!(
        name,
        "head"
            | "script"
            | "style"
            | "title"
            | "meta"
            | "link"
            | "template"
            | "noscript"
            | "object"
    )
}

/// 属性値を探す(名前空間つきの属性 `xlink:href` も、ローカル名 `href` で見つかる)。
fn attr<'a>(element: &'a ElementRef, name: &str) -> Option<&'a str> {
    element
        .value()
        .attrs()
        .find(|(n, _)| *n == name)
        .map(|(_, v)| v)
}

/// ラベルに使える名前にする。
fn sanitize_label(text: &str) -> String {
    text.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | ':' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// 変換の状態。
struct Converter<'a> {
    /// ZIPの中のパス → 章の番号。
    chapter_index: HashMap<&'a str, usize>,
    /// 今の章の番号とパス。
    current: usize,
    current_path: &'a str,
    /// まだ置いていないアンカー(次に出す断片の前に置く)。
    pending_anchors: Vec<String>,
    /// 置いたアンカー。
    emitted: HashSet<String>,
    /// 画像: ZIPの中のパス → 出力先。
    images: HashMap<String, String>,
    resources: Vec<(String, String)>,
    warnings: Vec<String>,
    available: &'a HashSet<String>,
}

/// リンクの行き先が決まる前の目印(最後に、置いたアンカーだけを有効にして置き換える)。
const LINK_OPEN: char = '\u{1}';
const LINK_CLOSE: char = '\u{2}';

impl<'a> Converter<'a> {
    fn anchor_name(&self, id: &str) -> String {
        format!("c{}-{}", self.current, sanitize_label(id))
    }

    fn add_anchor(&mut self, label: String) {
        self.pending_anchors.push(label);
    }

    /// 置くべきアンカーのコード。同じラベルは1度だけ。
    fn take_anchors(&mut self) -> Vec<String> {
        let labels = std::mem::take(&mut self.pending_anchors);
        labels
            .into_iter()
            .filter(|l| self.emitted.insert(l.clone()))
            .map(|l| format!("#box(width: 0pt, height: 0pt)<{l}>"))
            .collect()
    }

    /// 要素の`id`をアンカーにする。
    fn note_id(&mut self, element: &ElementRef) {
        if let Some(id) = element.value().id().filter(|i| !i.is_empty()) {
            let label = self.anchor_name(id);
            self.add_anchor(label);
        }
    }

    /// 画像を資源として登録して、出力先を返す。使えない画像は`None`。
    fn image(&mut self, src: &str) -> Option<String> {
        if src.starts_with("data:") || src.contains("://") {
            self.warnings
                .push("外部・埋め込みの画像は表示できません".into());
            return None;
        }
        let dir = self.current_path.rsplit_once('/').map_or("", |(d, _)| d);
        let path = resolve(dir, &percent_decode(src));
        let ext = path.rsplit_once('.')?.1.to_ascii_lowercase();
        if !IMAGE_EXTENSIONS.contains(&ext.as_str()) {
            self.warnings
                .push(format!("対応していない画像の形式です: {path}"));
            return None;
        }
        if !self.available.contains(&path) {
            self.warnings.push(format!("画像が見つかりません: {path}"));
            return None;
        }
        if let Some(out) = self.images.get(&path) {
            return Some(out.clone());
        }
        let out = format!("res/{}.{ext}", self.resources.len());
        self.images.insert(path.clone(), out.clone());
        self.resources.push((path, out.clone()));
        Some(out)
    }

    /// `href`を、リンクの開きのコードにする。行き先が無い・対応しないものは`None`。
    fn link_open(&self, href: &str) -> Option<String> {
        let href = href.trim();
        if href.is_empty() {
            return None;
        }
        let lower = href.to_ascii_lowercase();
        if lower.starts_with("http://")
            || lower.starts_with("https://")
            || lower.starts_with("mailto:")
        {
            return Some(format!("#link(\"{}\")", string_literal(href)));
        }
        if lower.starts_with("javascript:") || href.contains("://") {
            return None;
        }
        let (path, fragment) = href.split_once('#').unwrap_or((href, ""));
        let index = if path.is_empty() {
            self.current
        } else {
            let dir = self.current_path.rsplit_once('/').map_or("", |(d, _)| d);
            *self
                .chapter_index
                .get(resolve(dir, &percent_decode(path)).as_str())?
        };
        let label = if fragment.is_empty() {
            format!("c{index}")
        } else {
            format!("c{index}-{}", sanitize_label(&percent_decode(fragment)))
        };
        // 行き先が実際に置かれたかは、最後に決める。
        Some(format!("{LINK_OPEN}{label}{LINK_CLOSE}"))
    }

    // ---- 変換 ----

    /// `parent`の子を、ブロックの並びとして書く。
    fn blocks(&mut self, parent: ElementRef, out: &mut String, depth: usize) {
        let mut inline = Inline::default();
        for child in parent.children() {
            match child.value() {
                Node::Text(text) => inline.text(&collapse_whitespace(text)),
                Node::Element(_) => {
                    let element = ElementRef::wrap(child).expect("要素");
                    let name = element.value().name();
                    if is_skipped(name) {
                        continue;
                    }
                    if is_block(name) || name == "table" {
                        self.flush(&mut inline, out);
                        self.block(element, out, depth + 1);
                    } else {
                        self.inline_element(element, &mut inline, depth + 1);
                    }
                }
                _ => {}
            }
        }
        self.flush(&mut inline, out);
    }

    /// 溜まった文字を、段落として書き出す。
    fn flush(&mut self, inline: &mut Inline, out: &mut String) {
        let mut taken = std::mem::take(inline);
        taken.trim();
        if taken.is_blank() {
            // 目に見えるものがなくても、アンカーは残す(リンクの行き先になる)。
            for piece in &taken.pieces {
                if let Piece::Code(c) = piece {
                    out.push_str(c);
                    out.push('\n');
                }
            }
            return;
        }
        out.push_str(&taken.render());
        out.push_str("\n\n");
    }

    fn inline_of(&mut self, element: ElementRef, depth: usize) -> Inline {
        let mut inline = Inline::default();
        self.inline_children(element, &mut inline, depth);
        inline
    }

    fn inline_children(&mut self, parent: ElementRef, inline: &mut Inline, depth: usize) {
        if depth > MAX_DEPTH {
            self.warnings
                .push("入れ子が深すぎる部分を省きました".into());
            return;
        }
        for child in parent.children() {
            match child.value() {
                Node::Text(text) => {
                    self.place_anchors(inline);
                    inline.text(&collapse_whitespace(text));
                }
                Node::Element(_) => {
                    let element = ElementRef::wrap(child).expect("要素");
                    let name = element.value().name();
                    if is_skipped(name) {
                        continue;
                    }
                    if is_block(name) {
                        // インラインの中のブロックは、改行で区切って中身を続ける。
                        inline.code("#linebreak()");
                    }
                    self.inline_element(element, inline, depth + 1);
                }
                _ => {}
            }
        }
    }

    fn place_anchors(&mut self, inline: &mut Inline) {
        for anchor in self.take_anchors() {
            inline.code(anchor);
        }
    }

    /// 内側を`[...]`に包んで、`head`の呼び出しにする。
    fn wrapped(&mut self, head: &str, element: ElementRef, inline: &mut Inline, depth: usize) {
        let inner = self.inline_of(element, depth);
        inline.code(format!("{head}[{}]", inner.render()));
    }

    fn inline_element(&mut self, element: ElementRef, inline: &mut Inline, depth: usize) {
        self.note_id(&element);
        let name = element.value().name().to_string();
        match name.as_str() {
            "br" => {
                self.place_anchors(inline);
                inline.code("#linebreak()");
            }
            "em" | "i" | "cite" | "dfn" | "var" => {
                self.place_anchors(inline);
                self.wrapped("#emph", element, inline, depth);
            }
            "strong" | "b" => {
                self.place_anchors(inline);
                self.wrapped("#strong", element, inline, depth);
            }
            "u" | "ins" => {
                self.place_anchors(inline);
                self.wrapped("#underline", element, inline, depth);
            }
            "s" | "del" | "strike" => {
                self.place_anchors(inline);
                self.wrapped("#strike", element, inline, depth);
            }
            "mark" => {
                self.place_anchors(inline);
                self.wrapped("#highlight", element, inline, depth);
            }
            "sup" => {
                self.place_anchors(inline);
                self.wrapped("#super", element, inline, depth);
            }
            "sub" => {
                self.place_anchors(inline);
                self.wrapped("#sub", element, inline, depth);
            }
            "small" => {
                self.place_anchors(inline);
                self.wrapped("#text(size: 0.85em)", element, inline, depth);
            }
            "code" | "kbd" | "samp" | "tt" => {
                self.place_anchors(inline);
                let text: String = element.text().collect();
                inline.code(format!(
                    "#raw(\"{}\")",
                    string_literal(&collapse_whitespace(&text))
                ));
            }
            "q" => {
                self.place_anchors(inline);
                inline.text("\u{201c}");
                self.inline_children(element, inline, depth);
                inline.text("\u{201d}");
            }
            // ルビは、本文のあとに小さく括弧つきで添える(ルビ表示に対応するまでの代わり)。
            "rt" => {
                let text: String = element.text().collect();
                let text = collapse_whitespace(&text);
                let text = text.trim();
                if !text.is_empty() {
                    inline.code(format!(
                        "#text(size: 0.65em)[#\"({})\"]",
                        string_literal(text)
                    ));
                }
            }
            "rp" | "rtc" => {}
            "img" => {
                self.place_anchors(inline);
                let alt = attr(&element, "alt").unwrap_or("");
                match attr(&element, "src").and_then(|s| self.image(s)) {
                    Some(path) => inline.code(format!("#epub-image(\"{path}\")")),
                    None => inline.text(&collapse_whitespace(alt)),
                }
            }
            // 表紙などで使われるSVGの中の画像。
            "svg" => {
                self.place_anchors(inline);
                for child in element.descendants().filter_map(ElementRef::wrap) {
                    if child.value().name() == "image"
                        && let Some(path) = attr(&child, "href").and_then(|s| self.image(s))
                    {
                        inline.code(format!("#epub-image(\"{path}\")"));
                    }
                }
            }
            "a" => {
                self.place_anchors(inline);
                match attr(&element, "href").and_then(|h| self.link_open(h)) {
                    Some(open) => {
                        let inner = self.inline_of(element, depth);
                        inline.code(format!("{open}[{}]", inner.render()));
                    }
                    None => self.inline_children(element, inline, depth),
                }
            }
            _ => self.inline_children(element, inline, depth),
        }
    }

    fn block(&mut self, element: ElementRef, out: &mut String, depth: usize) {
        if depth > MAX_DEPTH {
            self.warnings
                .push("入れ子が深すぎる部分を省きました".into());
            return;
        }
        self.note_id(&element);
        let name = element.value().name().to_string();
        match name.as_str() {
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                let level = name[1..].parse::<usize>().unwrap_or(1);
                let mut inline = self.inline_of(element, depth);
                let anchors = self.take_anchors();
                inline.trim();
                if inline.is_blank() {
                    for a in anchors {
                        out.push_str(&a);
                        out.push('\n');
                    }
                    return;
                }
                out.push_str(&anchors.concat());
                out.push_str(&format!(
                    "#heading(level: {level})[{}]\n\n",
                    inline.render()
                ));
            }
            "ul" | "ol" => {
                let start = attr(&element, "start").and_then(|s| s.trim().parse::<i64>().ok());
                let items = self.list_items(element, depth);
                if items.is_empty() {
                    return;
                }
                let head = if name == "ol" {
                    match start {
                        Some(n) => format!("#enum(start: {n}, "),
                        None => "#enum(".to_string(),
                    }
                } else {
                    "#list(".to_string()
                };
                out.push_str(&format!("{head}{})\n\n", items.join(", ")));
            }
            "li" | "dd" | "figure" | "details" | "summary" | "div" | "section" | "article"
            | "aside" | "header" | "footer" | "main" | "nav" | "body" | "center" | "hgroup"
            | "address" | "figcaption" => self.blocks(element, out, depth),
            "dl" => self.definition_list(element, out, depth),
            "dt" => {
                let inner = self.inline_of(element, depth);
                out.push_str(&format!("#strong[{}]\n\n", inner.render()));
            }
            "p" => {
                let mut inline = self.inline_of(element, depth);
                self.flush(&mut inline, out);
            }
            "blockquote" => {
                let mut inner = String::new();
                self.blocks(element, &mut inner, depth);
                out.push_str(&format!("#quote(block: true)[\n{inner}]\n\n"));
            }
            "pre" => {
                let text: String = element.text().collect();
                out.push_str(&format!(
                    "#raw(block: true, \"{}\")\n\n",
                    string_literal(text.trim_matches('\n'))
                ));
            }
            "hr" => out.push_str("#line(length: 100%, stroke: 0.5pt)\n\n"),
            "table" => self.table(element, out, depth),
            _ => self.blocks(element, out, depth),
        }
    }

    /// `ul`/`ol`の項目を、`[...]`の並びにする。
    fn list_items(&mut self, list: ElementRef, depth: usize) -> Vec<String> {
        let mut items = Vec::new();
        for child in list.children().filter_map(ElementRef::wrap) {
            if child.value().name() == "li" {
                self.note_id(&child);
                let mut inner = String::new();
                self.blocks(child, &mut inner, depth + 1);
                items.push(format!("[\n{inner}]"));
            }
        }
        items
    }

    fn definition_list(&mut self, list: ElementRef, out: &mut String, depth: usize) {
        for child in list.children().filter_map(ElementRef::wrap) {
            match child.value().name() {
                "dt" => self.block(child, out, depth + 1),
                "dd" => {
                    let mut inner = String::new();
                    self.blocks(child, &mut inner, depth + 1);
                    out.push_str(&format!("#pad(left: 1.5em)[\n{inner}]\n\n"));
                }
                _ => {}
            }
        }
    }

    fn table(&mut self, table: ElementRef, out: &mut String, depth: usize) {
        let mut rows: Vec<Vec<String>> = Vec::new();
        let mut columns = 0;
        for row in table
            .descendants()
            .filter_map(ElementRef::wrap)
            .filter(|e| e.value().name() == "tr")
        {
            let mut cells = Vec::new();
            let mut width = 0;
            for cell in row.children().filter_map(ElementRef::wrap) {
                let name = cell.value().name();
                if name != "td" && name != "th" {
                    continue;
                }
                let span = |n: &str| {
                    attr(&cell, n)
                        .and_then(|v| v.trim().parse::<usize>().ok())
                        .filter(|v| *v >= 1)
                        .unwrap_or(1)
                };
                let (colspan, rowspan) = (span("colspan"), span("rowspan"));
                width += colspan;
                let mut inner = String::new();
                self.blocks(cell, &mut inner, depth + 1);
                if name == "th" {
                    inner = format!("#strong[\n{inner}]");
                }
                let options = match (colspan, rowspan) {
                    (1, 1) => String::new(),
                    (c, 1) => format!("colspan: {c}"),
                    (1, r) => format!("rowspan: {r}"),
                    (c, r) => format!("colspan: {c}, rowspan: {r}"),
                };
                cells.push(if options.is_empty() {
                    format!("[\n{inner}]")
                } else {
                    format!("table.cell({options})[\n{inner}]")
                });
            }
            columns = columns.max(width);
            if !cells.is_empty() {
                rows.push(cells);
            }
        }
        if columns == 0 {
            return;
        }
        let cells: Vec<String> = rows.into_iter().flatten().collect();
        out.push_str(&format!(
            "#table(columns: {columns}, stroke: 0.5pt, inset: 5pt, {})\n\n",
            cells.join(", ")
        ));
    }
}

/// 本の全章をTypstの本文に変換する。`available`は、ZIPの中にある(画像などの)ファイルのパス。
pub fn convert(chapters: &[Chapter], available: &HashSet<String>) -> Converted {
    let chapter_index: HashMap<&str, usize> = chapters
        .iter()
        .enumerate()
        .map(|(i, c)| (c.path.as_str(), i))
        .collect();
    let mut converter = Converter {
        chapter_index,
        current: 0,
        current_path: "",
        pending_anchors: Vec::new(),
        emitted: HashSet::new(),
        images: HashMap::new(),
        resources: Vec::new(),
        warnings: Vec::new(),
        available,
    };
    let body_selector = Selector::parse("body").expect("セレクタ");
    let mut body = String::new();
    for (index, chapter) in chapters.iter().enumerate() {
        converter.current = index;
        converter.current_path = &chapter.path;
        converter.pending_anchors.clear();
        converter.add_anchor(format!("c{index}"));
        let html = Html::parse_document(&chapter.html);
        let root = html
            .select(&body_selector)
            .next()
            .unwrap_or_else(|| html.root_element());
        let mut text = String::new();
        converter.blocks(root, &mut text, 0);
        // 置かれなかったアンカーも、章の終わりに置く(置いたつもりのリンクが切れないように)。
        for anchor in converter.take_anchors() {
            text.push_str(&anchor);
            text.push('\n');
        }
        body.push_str(&text);
        if index + 1 < chapters.len() {
            body.push_str("#pagebreak(weak: true)\n\n");
        }
    }
    let body = resolve_links(&body, &converter.emitted);
    Converted {
        body,
        resources: converter.resources,
        warnings: dedup(converter.warnings),
    }
}

/// リンクの目印を、行き先が置かれたものは`#link(<ラベル>)`に、置かれなかったものは素の`#text`にする。
fn resolve_links(body: &str, emitted: &HashSet<String>) -> String {
    let mut out = String::with_capacity(body.len());
    let mut rest = body;
    while let Some(start) = rest.find(LINK_OPEN) {
        out.push_str(&rest[..start]);
        let after = &rest[start + LINK_OPEN.len_utf8()..];
        let Some(end) = after.find(LINK_CLOSE) else {
            rest = after;
            continue;
        };
        let label = &after[..end];
        if emitted.contains(label) {
            out.push_str(&format!("#link(<{label}>)"));
        } else {
            out.push_str("#text");
        }
        rest = &after[end + LINK_CLOSE.len_utf8()..];
    }
    out.push_str(rest);
    out
}

fn dedup(warnings: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    warnings
        .into_iter()
        .filter(|w| seen.insert(w.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(html: &str) -> Converted {
        let chapters = [Chapter {
            path: "OEBPS/c0.xhtml".into(),
            html: html.into(),
        }];
        convert(&chapters, &HashSet::new())
    }

    fn body(html: &str) -> String {
        run(&format!("<html><body>{html}</body></html>")).body
    }

    #[test]
    fn string_literals_are_escaped() {
        assert_eq!(string_literal("a\"b\\c\nd"), "a\\\"b\\\\c\\nd");
        assert_eq!(string_literal("x\u{0}y"), "xy");
    }

    #[test]
    fn whitespace_collapses_but_japanese_line_breaks_vanish() {
        assert_eq!(collapse_whitespace("a  \n\t b"), "a b");
        assert_eq!(collapse_whitespace("日本\n語"), "日本語");
        assert_eq!(collapse_whitespace("日本 語"), "日本 語");
        assert_eq!(collapse_whitespace("abc\n日本"), "abc 日本");
    }

    #[test]
    fn a_paragraph_becomes_a_string_literal() {
        let b = body("<p>Hello  <em>big</em> world</p>");
        assert!(b.contains("#\"Hello \"#emph[#\"big\"]#\" world\""), "{b}");
    }

    /// マークアップの特殊文字は、文字列の中に入るので、そのまま文字として扱われる。
    #[test]
    fn markup_characters_stay_literal() {
        let b = body("<p>= not a heading * # $ - -- ~</p>");
        assert!(b.contains("#\"= not a heading * # $ - -- ~\""), "{b}");
    }

    #[test]
    fn headings_have_levels() {
        let b = body("<h1>One</h1><h3>Three</h3>");
        // 章の先頭のアンカーは、最初の見出しの中に入る(リンクの行き先として働く)。
        assert!(
            b.contains("#heading(level: 1)[") && b.contains("#\"One\"]"),
            "{b}"
        );
        assert!(b.contains("#heading(level: 3)[#\"Three\"]"), "{b}");
    }

    #[test]
    fn lists_nest() {
        let b = body("<ul><li>a<ul><li>b</li></ul></li></ul><ol start=\"3\"><li>c</li></ol>");
        assert!(b.contains("#list("), "{b}");
        assert!(b.matches("#list(").count() == 2, "{b}");
        assert!(b.contains("#enum(start: 3, "), "{b}");
    }

    #[test]
    fn external_links_are_links() {
        let b = body("<p><a href=\"https://example.com/?a=1&amp;b=2\">site</a></p>");
        assert!(
            b.contains("#link(\"https://example.com/?a=1&b=2\")[#\"site\"]"),
            "{b}"
        );
    }

    /// 本の中のリンクは、行き先が置かれていれば有効、無ければ素の文字になる。
    #[test]
    fn internal_links_need_their_target() {
        let chapters = [
            Chapter {
                path: "OEBPS/a.xhtml".into(),
                html: "<body><p><a href=\"b.xhtml#x\">ok</a> <a href=\"b.xhtml#nope\">lost</a> <a href=\"c.xhtml\">gone</a></p></body>".into(),
            },
            Chapter {
                path: "OEBPS/b.xhtml".into(),
                html: "<body><h2 id=\"x\">Target</h2></body>".into(),
            },
        ];
        let b = convert(&chapters, &HashSet::new()).body;
        assert!(b.contains("#link(<c1-x>)[#\"ok\"]"), "{b}");
        assert!(b.contains("#text[#\"lost\"]"), "{b}");
        // 存在しない章へのリンクは、リンクにせず文字だけ残す。
        assert!(b.contains("gone") && !b.contains("<c2"), "{b}");
        assert!(b.contains("<c1-x>"), "{b}");
        assert!(!b.contains(LINK_OPEN) && !b.contains(LINK_CLOSE));
    }

    #[test]
    fn images_become_numbered_resources() {
        let chapters = [Chapter {
            path: "OEBPS/text/a.xhtml".into(),
            html: "<body><p><img src=\"../img/pic%201.png\" alt=\"pic\"/><img src=\"../img/pic%201.png\"/><img src=\"missing.png\" alt=\"alt text\"/></p></body>".into(),
        }];
        let available: HashSet<String> = ["OEBPS/img/pic 1.png".to_string()].into();
        let c = convert(&chapters, &available);
        assert_eq!(
            c.resources,
            [("OEBPS/img/pic 1.png".to_string(), "res/0.png".to_string())]
        );
        assert_eq!(
            c.body.matches("#epub-image(\"res/0.png\")").count(),
            2,
            "{}",
            c.body
        );
        assert!(c.body.contains("#\"alt text\""), "{}", c.body);
        assert!(c.warnings.iter().any(|w| w.contains("見つかりません")));
    }

    #[test]
    fn svg_cover_images_are_found() {
        let chapters = [Chapter {
            path: "c.xhtml".into(),
            html: "<body><svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\"><image xlink:href=\"cover.jpg\"/></svg></body>".into(),
        }];
        let available: HashSet<String> = ["cover.jpg".to_string()].into();
        let c = convert(&chapters, &available);
        assert!(c.body.contains("#epub-image(\"res/0.jpg\")"), "{}", c.body);
    }

    #[test]
    fn ruby_text_follows_the_base_in_parentheses() {
        let b = body("<p><ruby>漢字<rp>(</rp><rt>かんじ</rt><rp>)</rp></ruby></p>");
        assert!(b.contains("#\"漢字\""), "{b}");
        assert!(b.contains("#text(size: 0.65em)[#\"(かんじ)\"]"), "{b}");
        assert!(!b.contains("#\"(\"#"), "rp should be ignored: {b}");
    }

    #[test]
    fn tables_have_columns_and_spans() {
        let b = body(
            "<table><tr><th>A</th><th>B</th></tr><tr><td colspan=\"2\">wide</td></tr></table>",
        );
        assert!(b.contains("#table(columns: 2"), "{b}");
        assert!(b.contains("table.cell(colspan: 2)"), "{b}");
    }

    #[test]
    fn pre_keeps_its_newlines() {
        let b = body("<pre>line 1\n  line 2</pre>");
        assert!(
            b.contains("#raw(block: true, \"line 1\\n  line 2\")"),
            "{b}"
        );
    }

    #[test]
    fn scripts_and_styles_are_ignored() {
        let b = body("<p>x</p><script>alert(1)</script><style>p{}</style>");
        assert!(!b.contains("alert") && !b.contains("p{}"), "{b}");
    }

    #[test]
    fn chapters_are_separated_by_page_breaks() {
        let chapters = [
            Chapter {
                path: "a".into(),
                html: "<body><p>1</p></body>".into(),
            },
            Chapter {
                path: "b".into(),
                html: "<body><p>2</p></body>".into(),
            },
        ];
        let b = convert(&chapters, &HashSet::new()).body;
        assert_eq!(b.matches("#pagebreak(weak: true)").count(), 1);
    }

    #[test]
    fn deeply_nested_input_does_not_overflow() {
        let html = format!("{}x{}", "<div>".repeat(5000), "</div>".repeat(5000));
        let c = run(&format!("<body>{html}</body>"));
        assert!(!c.body.is_empty() || !c.warnings.is_empty());
    }
}
