# 開発の軌跡 (Journey)

Zed向けGPUI製PDFビューアの開発過程を記録する。

## 全体方針
- Cargoワークスペースで3crate構成: `pdf_core` / `pdf_viewer` / `pdf_view_app`
- 手順: ①雛形 → ②pdf_core → ③pdf_viewer(1ページ) → ④アプリ表示 → ⑤スクロール+ズーム
- 着手前に方針確認、1関数ごとにコミット、3〜5コミットでリファクタ、pushは確認後

## ① ワークスペース雛形
- 方針を提示し承認を得た。
- Rust toolchain: Zed本体の `rust-toolchain.toml` (rev fae8a35 時点で 1.98.1) に合わせて固定した。手元の既定は 1.95.0 だったが、rustup が 1.98.1 を取得した。
- edition は 2024。共通設定は `[workspace.package]` に集約。
- GPUIのrev候補: `fae8a354829e56bf5615fedfa8cdfecb124550c4` (zed main HEAD, 2026-10-21 取得)。pdf_viewer の依存追加時(③)に固定する。
- 各crateは空の雛形で `cargo check` が通ることを確認。
