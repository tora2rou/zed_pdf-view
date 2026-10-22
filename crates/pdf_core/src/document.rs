use std::{
    collections::HashMap,
    fs,
    path::Path,
    sync::{Arc, OnceLock},
};

use hayro::{
    PixmapSettings, RenderCache, RenderSettings,
    hayro_interpret::InterpreterSettings,
    hayro_syntax::{Pdf, object::ObjectIdentifier, page::Page},
    vello_cpu::color::palette::css::WHITE,
};

use crate::{
    Link, OutlineItem, PageText, PdfError, RenderedPage, dest::page_index_by_id, links::read_links,
    outline::read_outline, text::extract_text,
};

/// PDFファイルの中身。メモリマップで読み、必要なページだけをOSに読み込ませる
/// (全体を読み込むより常駐メモリが少なく、足りなくなればOSが捨てて読み直せる)。
/// マップできないとき(空ファイルなど)は普通に読む。
enum FileData {
    Mapped(memmap2::Mmap),
    Read(Vec<u8>),
}

impl AsRef<[u8]> for FileData {
    fn as_ref(&self) -> &[u8] {
        match self {
            Self::Mapped(map) => map,
            Self::Read(bytes) => bytes,
        }
    }
}

fn read_file(path: &Path) -> Result<Arc<FileData>, PdfError> {
    let file = fs::File::open(path)?;
    // SAFETY: 開いている間に他のプログラムがファイルを短くすると不正なアクセスになりうる。
    // PDFビューアとして許容している(Windowsでは、開いている間は上書きできない)。
    let data = match unsafe { memmap2::Mmap::map(&file) } {
        Ok(map) => FileData::Mapped(map),
        Err(_) => FileData::Read(fs::read(path)?),
    };
    Ok(Arc::new(data))
}

pub struct PdfDocument {
    pdf: Pdf,
    /// 目次。初めて必要になったときに読む。
    outline: OnceLock<Vec<OutlineItem>>,
    /// ページオブジェクトの識別子 → ページ番号。目次やリンクの宛先の解決に使う。
    page_ids: OnceLock<HashMap<ObjectIdentifier, usize>>,
    /// ページごとのリンク。そのページを初めて調べたときに読む。
    links: Vec<OnceLock<Vec<Link>>>,
    /// ページごとの文字。そのページを初めて調べたときに取り出す。
    texts: Vec<OnceLock<PageText>>,
}

impl PdfDocument {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, PdfError> {
        let pdf =
            Pdf::new(read_file(path.as_ref())?).map_err(|e| PdfError::Load(format!("{e:?}")))?;
        let page_count = pdf.pages().len();
        Ok(Self {
            pdf,
            outline: OnceLock::new(),
            page_ids: OnceLock::new(),
            links: (0..page_count).map(|_| OnceLock::new()).collect(),
            texts: (0..page_count).map(|_| OnceLock::new()).collect(),
        })
    }

    pub fn page_count(&self) -> usize {
        self.pdf.pages().len()
    }

    /// 文書の目次。無ければ空。初回だけ読み込む。
    pub fn outline(&self) -> &[OutlineItem] {
        self.outline
            .get_or_init(|| read_outline(&self.pdf, self.page_ids()))
    }

    /// ページのサイズ(幅, 高さ)をPDFポイント単位(1/72インチ)で返す。回転・CropBox反映済み。
    pub fn page_size(&self, index: usize) -> Result<(f32, f32), PdfError> {
        Ok(self.page(index)?.render_dimensions())
    }

    /// ページを`scale`倍(1.0 = 72dpi)でRGBAにラスタライズする。
    /// 結果は保持しない(画像は呼び出し側が持つので、ここで持つとメモリが二重になる)。
    pub fn render_page(&self, index: usize, scale: f32) -> Result<RenderedPage, PdfError> {
        self.rasterize(index, scale)
    }

    fn rasterize(&self, index: usize, scale: f32) -> Result<RenderedPage, PdfError> {
        let page = self.page(index)?;
        let (w, h) = page.render_dimensions();
        validate_scale(scale, (w, h))?;
        let pixmap = hayro::render(
            page,
            &RenderCache::new(),
            &InterpreterSettings::default(),
            &RenderSettings::default(),
            &PixmapSettings {
                x_scale: scale,
                y_scale: scale,
                bg_color: WHITE,
            },
        );
        Ok(RenderedPage::from_pixmap(pixmap))
    }

    /// ページ`index`のリンク。初回だけ読み込む。
    pub fn page_links(&self, index: usize) -> Result<&[Link], PdfError> {
        let cell = self.links.get(index).ok_or(PdfError::PageOutOfRange {
            index,
            count: self.links.len(),
        })?;
        Ok(cell.get_or_init(|| {
            let empty = PageText::default();
            let text = self.page_text(index).unwrap_or(&empty);
            read_links(&self.pdf, self.page_ids(), index, text)
        }))
    }

    /// ページ`index`のリンクが、すでに読み込み済みならそれを返す。読み込み中や未読み込みなら`None`で、
    /// 待たずに戻る(画面を更新するスレッドで、重い読み込みを待たないために使う)。
    pub fn page_links_if_ready(&self, index: usize) -> Option<&[Link]> {
        self.links.get(index)?.get().map(Vec::as_slice)
    }

    /// ページ`index`の文字が取り出し済みならそれを返す。未取り出しなら`None`で、待たずに戻る
    /// (画面を更新するスレッドで、重い取り出しを待たないために使う)。
    pub fn page_text_if_ready(&self, index: usize) -> Option<&PageText> {
        self.texts.get(index)?.get()
    }

    /// ページ`index`の文字と位置。初回だけ取り出す(ページの解釈が要るので、重いことがある)。
    pub fn page_text(&self, index: usize) -> Result<&PageText, PdfError> {
        let cell = self.texts.get(index).ok_or(PdfError::PageOutOfRange {
            index,
            count: self.texts.len(),
        })?;
        Ok(cell.get_or_init(|| {
            self.pdf
                .pages()
                .get(index)
                .map(extract_text)
                .unwrap_or_default()
        }))
    }

    fn page_ids(&self) -> &HashMap<ObjectIdentifier, usize> {
        self.page_ids.get_or_init(|| page_index_by_id(&self.pdf))
    }

    fn page(&self, index: usize) -> Result<&Page<'_>, PdfError> {
        let pages = self.pdf.pages();
        pages.get(index).ok_or(PdfError::PageOutOfRange {
            index,
            count: pages.len(),
        })
    }
}

/// 倍率が正の有限値で、描画後の画像サイズがhayroの上限(`u16`)に収まることを確かめる。
fn validate_scale(scale: f32, (width, height): (f32, f32)) -> Result<(), PdfError> {
    let max = f32::from(u16::MAX);
    if scale.is_finite() && scale > 0.0 && width * scale < max && height * scale < max {
        Ok(())
    } else {
        Err(PdfError::InvalidScale(scale))
    }
}
