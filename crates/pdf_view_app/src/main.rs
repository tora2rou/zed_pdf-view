// Windowsのreleaseビルドでは、起動時にコンソール(黒い画面)を出さない。
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app_window;
mod logger;
mod recent;
mod recent_menu;
mod resize;
mod tab_bar;
mod titlebar;

use std::{path::PathBuf, process::ExitCode, sync::Arc};

use gpui::{App, AppContext as _, Bounds, WindowBounds, WindowOptions, px, size};
use gpui_platform::application;
use pdf_core::PdfDocument;

use app_window::{APP_NAME, AppWindow};

/// デスクトップ環境がウィンドウを識別する名前。`packaging/linux/io.github.tora2rou.Chihayaburu.desktop`のファイル名と同じ。
const APP_ID: &str = "io.github.tora2rou.Chihayaburu";

const USAGE: &str = "使い方: pdf_view_app [file.pdf]";

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

/// 起動時に開くもの。開けなかったときは、そのエラーをウィンドウに出す。
struct Startup {
    document: Option<(PathBuf, Arc<PdfDocument>)>,
    error: Option<String>,
}

fn open_window(startup: Startup, cx: &mut App) {
    let bounds = Bounds::centered(None, size(px(900.), px(1000.)), cx);
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: Some(gpui::TitlebarOptions {
            title: Some(APP_NAME.into()),
            ..Default::default()
        }),
        // 枠(タイトルバー)は自前で描画する。
        window_decorations: Some(gpui::WindowDecorations::Client),
        // Linuxで、ウィンドウと`.desktop`(アイコン・名前)を結びつける。
        // Waylandのapp_id / X11のWM_CLASSになり、`.desktop`のファイル名と一致させる。
        app_id: Some(APP_ID.into()),
        ..Default::default()
    };
    let Startup { document, error } = startup;
    cx.open_window(options, |window, cx| {
        cx.new(|cx| {
            let mut app = AppWindow::new(document, recent::recent_files_path(), window, cx);
            if let Some(error) = error {
                app.show_error(error, cx);
            }
            app
        })
    })
    .expect("ウィンドウを開けませんでした");
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
    // 起動時に指定されたファイルが開けないときも、ウィンドウは開いてエラーを表示する
    // (エクスプローラーから開いたときなど、端末が無くてもエラーが分かるように)。
    let startup = match path {
        Some(path) => match PdfDocument::open(&path) {
            Ok(document) => Startup {
                document: Some((path, Arc::new(document))),
                error: None,
            },
            Err(e) => {
                let message = format!("{}: {e}", path.display());
                eprintln!("{message}");
                Startup {
                    document: None,
                    error: Some(message),
                }
            }
        },
        None => Startup {
            document: None,
            error: None,
        },
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
