use std::io;

#[derive(Debug, thiserror::Error)]
pub enum PdfError {
    #[error("ファイルを読み込めません: {0}")]
    Io(#[from] io::Error),
    #[error("PDFを解析できません: {0}")]
    Load(String),
    #[error("ページ番号が範囲外です: {index} (全{count}ページ)")]
    PageOutOfRange { index: usize, count: usize },
    #[error("ズーム倍率が不正です: {0}")]
    InvalidScale(f32),
    #[error("OCRに失敗しました: {0}")]
    Ocr(String),
}
