/// ラスタライズ済みのページ。`rgba`は白背景で塗られた不透明なRGBA8(行優先、隙間なし)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedPage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}
