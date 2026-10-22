mod app_window;
mod editor;
mod logger;
mod recent;
mod recent_menu;
mod reload;
mod resize;
mod source_tab;
mod tab_bar;
mod titlebar;

use std::{path::PathBuf, process::ExitCode, sync::Arc};

use gpui::{App, AppContext as _, Bounds, WindowBounds, WindowOptions, px, size};
use gpui_platform::application;
use pdf_core::PdfDocument;

use app_window::AppWindow;

const USAGE: &str = "使い方: pdf_view_app [file.pdf | file.typ | file.tex]";

/// コマンドライン引数(プログラム名を除く)から、開くPDFのパスを取り出す。
/// 引数がなければ`None`(PDFを開かずに起動する)。
fn pdf_path_from_args(mut args: impl Iterator<Item = String>) -> Result<Option<PathBuf>, String> {
    let Some(path) = args.next() else {
        return Ok(None);
    };
    if args.next().is_some() {
        return Err(USAGE.to_string());
    }
    Ok(Some(PathBuf::from(path)))
}

/// 起動時に開くもの。
enum Startup {
    /// 読み込み済みのPDF。
    Pdf(PathBuf, Arc<PdfDocument>),
    /// Typst・LaTeXの文書。ウィンドウを開いてからコンパイルする(重いので、ウィンドウは先に出す)。
    Source(PathBuf),
}

fn open_window(startup: Option<Startup>, cx: &mut App) {
    let bounds = Bounds::centered(None, size(px(900.), px(1000.)), cx);
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: Some(gpui::TitlebarOptions {
            title: Some("PDF Viewer".into()),
            ..Default::default()
        }),
        // 枠(タイトルバー)は自前で描画する。
        window_decorations: Some(gpui::WindowDecorations::Client),
        ..Default::default()
    };
    let (initial, source) = match startup {
        Some(Startup::Pdf(path, document)) => (Some((path, document)), None),
        Some(Startup::Source(path)) => (None, Some(path)),
        None => (None, None),
    };
    let window = cx
        .open_window(options, |window, cx| {
            cx.new(|cx| AppWindow::new(initial, recent::recent_files_path(), window, cx))
        })
        .expect("ウィンドウを開けませんでした");
    if let Some(path) = source {
        window
            .update(cx, |app, window, cx| app.open_path(path, window, cx))
            .ok();
    }
    cx.activate(true);
}

fn main() -> ExitCode {
    logger::init();
    let path = match pdf_path_from_args(std::env::args().skip(1)) {
        Ok(path) => path,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::FAILURE;
        }
    };
    // 起動時に指定されたファイルが開けないときは、ウィンドウを出さずにエラー終了する。
    let startup = match path {
        // Typst・LaTeXの文書は、コンパイルに失敗してもエラーつきで開くので、ここでは読まない。
        Some(path) if source_tab::is_source(&path) => Some(Startup::Source(path)),
        Some(path) => match PdfDocument::open(&path) {
            Ok(document) => Some(Startup::Pdf(path, Arc::new(document))),
            Err(e) => {
                eprintln!("{}: {e}", path.display());
                return ExitCode::FAILURE;
            }
        },
        None => None,
    };
    application().run(move |cx: &mut App| {
        pdf_viewer::init(cx);
        app_window::init(cx);
        open_window(startup, cx);
    });
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> impl Iterator<Item = String> {
        list.iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
            .into_iter()
    }

    #[test]
    fn single_argument_is_the_path() {
        assert_eq!(
            pdf_path_from_args(args(&["a.pdf"])),
            Ok(Some(PathBuf::from("a.pdf")))
        );
    }

    #[test]
    fn no_argument_opens_without_a_file() {
        assert_eq!(pdf_path_from_args(args(&[])), Ok(None));
    }

    #[test]
    fn extra_arguments_show_usage() {
        assert_eq!(
            pdf_path_from_args(args(&["a.pdf", "b.pdf"])),
            Err(USAGE.to_string())
        );
    }
}
