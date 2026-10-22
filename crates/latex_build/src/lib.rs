//! LaTeX文書をPDFにコンパイルする(外部コマンドの`latexmk`を使う)。GUIには依存しない。

mod engine;
mod fls;
mod log;

pub use engine::{Engine, detect};
