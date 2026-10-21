//! ページ上の文字と、その位置の取り出し。

use hayro::{
    hayro_interpret::{
        BlendMode, ClipPath, Context, Device, DrawMode, DrawProps, Image, ImageDrawProps,
        InterpreterCache, InterpreterSettings, SoftMask, TransformExt, font::GlyphRun,
        interpret_page,
    },
    hayro_syntax::page::Page,
    kurbo::{Affine, BezPath, Point, Rect},
};

use crate::links::LinkRect;

/// 1ページから取り出す文字数の上限。壊れた/悪意あるPDFへの安全弁。
const MAX_CHARS: usize = 400_000;
/// 文字の箱の、ベースラインから下(ディセンダ)と上(アセンダ)の目安。グリフの座標系は1em = 1000。
const DESCENT: f64 = -210.0;
const ASCENT: f64 = 810.0;
/// 送り幅が分からないグリフの仮の送り幅。
const FALLBACK_ADVANCE: f64 = 500.0;
/// 前の文字の終わりと次の文字の始まりのずれが、これ以内なら「隣り合っている」とみなす(字間の調整など)。
const ADJACENT_TOLERANCE: f64 = 150.0;
/// 次の文字のベースラインが、これ以上ずれていたら別の行とみなす。
const SAME_LINE_TOLERANCE: f64 = 300.0;

/// 直前の文字との区切り。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gap {
    /// 隣り合っている(同じ語の続き)。
    None,
    /// 同じ行で、間があいている(語の区切り)。
    Space,
    /// 別の行(または、同じ行でも戻っている)。
    Line,
}

/// 1文字(ligatureなどは複数文字の文字列)と、ページ上の位置。
#[derive(Debug, Clone, PartialEq)]
pub struct TextChar {
    pub text: String,
    /// 表示される画像と同じ座標系(倍率1.0、原点左上、ポイント)の外接矩形。
    pub rect: LinkRect,
    /// 直前の文字との区切り。ページの最初の文字は`Line`。
    pub gap: Gap,
}

/// ページの文字。PDFの描画順(通常は読む順)に並ぶ。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PageText {
    pub chars: Vec<TextChar>,
}

/// 描画の様子から、文字だけを集める`Device`。
struct TextCollector {
    chars: Vec<TextChar>,
    /// 直前の文字の、グリフ座標 → 表示座標の変換と、送り幅。
    previous: Option<(Affine, f64)>,
}

impl TextCollector {
    fn gap_from_previous(&self, origin: Point) -> Gap {
        let Some((previous, advance)) = self.previous else {
            return Gap::Line;
        };
        if previous.determinant().abs() < 1e-12 {
            return Gap::Line;
        }
        // 直前の文字を基準にした、次の文字の始まりの位置(グリフ座標)。
        let local = previous.inverse() * origin;
        // 別の行、または同じ行でも戻っている(新しいテキストが先頭から始まるなど)。
        if local.y.abs() > SAME_LINE_TOLERANCE || local.x < advance - ADJACENT_TOLERANCE {
            Gap::Line
        } else if local.x <= advance + ADJACENT_TOLERANCE {
            Gap::None
        } else {
            Gap::Space
        }
    }
}

impl<'a> Device<'a> for TextCollector {
    fn draw_glyph_run(&mut self, run: &GlyphRun<'_, 'a>, props: DrawProps<'a>, _: &DrawMode) {
        for glyph in run.glyphs() {
            if self.chars.len() >= MAX_CHARS {
                return;
            }
            let Some(unicode) = glyph.as_unicode() else {
                continue;
            };
            let matrix = props.transform * glyph.transform();
            let advance = match &**glyph {
                hayro::hayro_interpret::font::Glyph::Outline(outline) => {
                    outline.advance_width().map_or(FALLBACK_ADVANCE, f64::from)
                }
                _ => FALLBACK_ADVANCE,
            };
            let text = match unicode {
                hayro::hayro_interpret::hayro_cmap::BfString::Char(c) => c.to_string(),
                hayro::hayro_interpret::hayro_cmap::BfString::String(s) => s,
            };
            let Some(rect) = glyph_rect(matrix, advance) else {
                continue;
            };
            let gap = self.gap_from_previous(matrix * Point::ORIGIN);
            self.chars.push(TextChar { text, rect, gap });
            self.previous = Some((matrix, advance));
        }
    }

    fn draw_path(&mut self, _: &BezPath, _: DrawProps<'a>, _: &DrawMode) {}
    fn push_clip_path(&mut self, _: &ClipPath) {}
    fn push_transparency_group(&mut self, _: f32, _: Option<SoftMask<'a>>, _: BlendMode) {}
    fn draw_image(&mut self, _: Image<'a, '_>, _: ImageDrawProps<'a>) {}
    fn pop_clip(&mut self) {}
    fn pop_transparency_group(&mut self) {}
}

/// グリフの箱(送り幅 × アセンダ/ディセンダ)を表示座標に直した、外接矩形。有限でなければ`None`。
fn glyph_rect(matrix: Affine, advance: f64) -> Option<LinkRect> {
    let corners = [
        Point::new(0.0, DESCENT),
        Point::new(advance, DESCENT),
        Point::new(0.0, ASCENT),
        Point::new(advance, ASCENT),
    ];
    let mut bounds = Rect::new(
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    );
    for corner in corners {
        let p = matrix * corner;
        if !p.x.is_finite() || !p.y.is_finite() {
            return None;
        }
        bounds = Rect::new(
            bounds.x0.min(p.x),
            bounds.y0.min(p.y),
            bounds.x1.max(p.x),
            bounds.y1.max(p.y),
        );
    }
    Some(LinkRect {
        x: bounds.x0 as f32,
        y: bounds.y0 as f32,
        width: (bounds.x1 - bounds.x0) as f32,
        height: (bounds.y1 - bounds.y0) as f32,
    })
}

/// ページの文字を、PDFの描画順に取り出す。
pub(crate) fn extract_text(page: &Page<'_>) -> PageText {
    let (width, height) = page.render_dimensions();
    let cache = InterpreterCache::new();
    let mut context = Context::new(
        page.initial_transform(true).to_kurbo(),
        Rect::new(0.0, 0.0, f64::from(width), f64::from(height)),
        &cache,
        page.xref(),
        InterpreterSettings::default(),
    );
    let mut collector = TextCollector {
        chars: Vec::new(),
        previous: None,
    };
    interpret_page(page, &mut context, &mut collector);
    PageText {
        chars: collector.chars,
    }
}

impl PageText {
    /// 文字を並べた文字列。語の区切りには空白、行の区切りには改行を入れる。
    pub fn to_plain_string(&self) -> String {
        let mut out = String::new();
        for (i, c) in self.chars.iter().enumerate() {
            if i > 0 {
                match c.gap {
                    Gap::None => {}
                    Gap::Space => out.push(' '),
                    Gap::Line => out.push('\n'),
                }
            }
            out.push_str(&c.text);
        }
        out
    }
}
