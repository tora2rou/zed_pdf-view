//! Windows向けにビルドするとき、`.exe` にアイコンと製品情報を埋め込む。
//! GPUIは、`.exe` のID 1のアイコンをウィンドウ(タイトルバー・タスクバー)にも使う。

fn main() {
    println!("cargo:rerun-if-changed=assets/chihayaburu.ico");
    // ビルドスクリプトはビルドする側(ホスト)で動く。Windows上でWindows向けにビルドするときだけ埋め込む。
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut resource = winresource::WindowsResource::new();
        resource
            .set_icon("assets/chihayaburu.ico")
            .set("ProductName", "Chihayaburu")
            .set("FileDescription", "Chihayaburu PDF Viewer")
            .set("OriginalFilename", "chihayaburu.exe");
        resource
            .compile()
            .expect("Windowsのリソース(アイコン)を埋め込めません");
    }
}
