use hayro::vello_cpu::Pixmap;

/// ラスタライズ済みのページ。`rgba`は白背景で塗られた不透明なRGBA8(行優先、隙間なし)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedPage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl RenderedPage {
    /// 不透明な背景で描画済みのPixmapから作る。
    /// 背景が不透明なのでpremultipliedでもstraightでも値は同じ。
    pub(crate) fn from_pixmap(pixmap: &Pixmap) -> Self {
        Self {
            width: u32::from(pixmap.width()),
            height: u32::from(pixmap.height()),
            rgba: pixmap.data_as_u8_slice().to_vec(),
        }
    }
}
