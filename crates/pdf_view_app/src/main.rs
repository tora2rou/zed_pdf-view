use std::{path::PathBuf, process::ExitCode, sync::Arc};

use gpui::{App, AppContext as _, Bounds, WindowBounds, WindowOptions, px, size};
use gpui_platform::application;
use pdf_core::PdfDocument;
use pdf_viewer::PdfView;

const USAGE: &str = "使い方: pdf_view_app <file.pdf>";

/// コマンドライン引数(プログラム名を除く)から、開くPDFのパスを取り出す。
fn pdf_path_from_args(mut args: impl Iterator<Item = String>) -> Result<PathBuf, String> {
    let path = args.next().ok_or_else(|| USAGE.to_string())?;
    if args.next().is_some() {
        return Err(USAGE.to_string());
    }
    Ok(PathBuf::from(path))
}

fn open_window(document: Arc<PdfDocument>, title: String, cx: &mut App) {
    let bounds = Bounds::centered(None, size(px(900.), px(1000.)), cx);
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: Some(gpui::TitlebarOptions {
            title: Some(title.into()),
            ..Default::default()
        }),
        ..Default::default()
    };
    cx.open_window(options, |_, cx| cx.new(|cx| PdfView::new(document, cx)))
        .expect("ウィンドウを開けませんでした");
    cx.activate(true);
}

fn main() -> ExitCode {
    let path = match pdf_path_from_args(std::env::args().skip(1)) {
        Ok(path) => path,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::FAILURE;
        }
    };
    let document = match PdfDocument::open(&path) {
        Ok(document) => Arc::new(document),
        Err(e) => {
            eprintln!("{}: {e}", path.display());
            return ExitCode::FAILURE;
        }
    };
    let title = path
        .file_name()
        .map_or_else(|| "PDF".into(), |n| n.to_string_lossy().into_owned());
    application().run(move |cx: &mut App| open_window(document, title, cx));
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
            Ok(PathBuf::from("a.pdf"))
        );
    }

    #[test]
    fn no_argument_shows_usage() {
        assert_eq!(pdf_path_from_args(args(&[])), Err(USAGE.to_string()));
    }

    #[test]
    fn extra_arguments_show_usage() {
        assert_eq!(
            pdf_path_from_args(args(&["a.pdf", "b.pdf"])),
            Err(USAGE.to_string())
        );
    }
}
