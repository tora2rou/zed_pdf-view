//! Typstの文書(`.typ`)を開くタブ。コンパイルしたPDFを表示し、エラーを下に出す。

use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

use build_diag::{Diagnostic, Level};
use gpui::{ClickEvent, Context, IntoElement, div, prelude::*, px, rgb};
use pdf_core::PdfDocument;

use crate::{app_window::AppWindow, editor};

const PANEL_BACKGROUND: u32 = 0x1e1e1e;
const PANEL_BORDER: u32 = 0x555555;
const ERROR_TEXT: u32 = 0xff7b72;
const WARNING_TEXT: u32 = 0xd29922;
const HINT_TEXT: u32 = 0x999999;
const ROW_HOVER: u32 = 0x2c2c2c;
const PANEL_MAX_HEIGHT: f32 = 160.0;

/// コンパイルしたPDFを残しておく数。表示中のものと、その前のもの。それより古いものは消す。
const KEEP_OUTPUTS: usize = 3;

/// Typstの文書を開いているタブの情報。
#[derive(Default)]
pub struct TypstTab {
    /// 直近のコンパイルのエラーと警告。
    pub diagnostics: Vec<Diagnostic>,
}

/// `.typ`ファイルか。
pub fn is_typst(path: &Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("typ"))
}

/// コンパイルの結果。
pub struct Build {
    /// コンパイルしたPDF。コンパイルに失敗したときは`None`。
    pub document: Option<PdfDocument>,
    /// エラーと警告。
    pub diagnostics: Vec<Diagnostic>,
    /// 読み込んだファイル。これらが書き換わったら、コンパイルし直す。
    pub dependencies: Vec<PathBuf>,
}

/// `source`をコンパイルして、PDFとして開く。重い処理なので、バックグラウンドで呼ぶ。
pub fn build(source: &Path) -> Build {
    match typst_build::compile_file(source) {
        Ok(compiled) => {
            let mut diagnostics = compiled.warnings;
            let document = match open_pdf(&compiled.pdf) {
                Ok(document) => Some(document),
                Err(message) => {
                    diagnostics.push(internal_error(message));
                    None
                }
            };
            Build {
                document,
                diagnostics,
                dependencies: compiled.dependencies,
            }
        }
        Err(failure) => Build {
            document: None,
            diagnostics: failure.errors.into_iter().chain(failure.warnings).collect(),
            dependencies: failure.dependencies,
        },
    }
}

fn internal_error(message: String) -> Diagnostic {
    Diagnostic {
        severity: Level::Error,
        message,
        file: None,
        path: None,
        line: None,
        column: None,
        hints: Vec::new(),
    }
}

/// コンパイルしたPDFの置き場所。起動ごと(プロセスごと)のフォルダ。
fn output_dir() -> PathBuf {
    std::env::temp_dir().join(format!("chihayaburu-typst-{}", std::process::id()))
}

/// 書き出したPDFのうち、まだ消していないもの(古い順)。
static OUTPUTS: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

/// PDFのバイト列をファイルに書いて、開く。ファイルは毎回別の名前にする(開いている文書が、
/// 同じ名前のファイルを上書きされないように)。
fn open_pdf(bytes: &[u8]) -> Result<PdfDocument, String> {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let dir = output_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("PDFの置き場所を作れません: {e}"))?;
    let path = dir.join(format!(
        "out-{}.pdf",
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    fs::write(&path, bytes).map_err(|e| format!("PDFを書き出せません: {e}"))?;
    let document = PdfDocument::open(&path).map_err(|e| e.to_string());
    forget_old_outputs(path);
    document
}

/// `latest`を記録して、古いものを消す。消せなくても(Windowsで開いているときなど)気にしない。
fn forget_old_outputs(latest: PathBuf) {
    let Ok(mut outputs) = OUTPUTS.lock() else {
        return;
    };
    outputs.push(latest);
    while outputs.len() > KEEP_OUTPUTS {
        let old = outputs.remove(0);
        let _ = fs::remove_file(old);
    }
}

/// 1ページだけの空白のPDF。最初のコンパイルが失敗したときに、エラーの表示の背景として見せる。
fn blank_pdf() -> Vec<u8> {
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] >>",
    ];
    let mut out = String::from("%PDF-1.4\n");
    let mut offsets = Vec::new();
    for (i, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.push_str(&format!("{} 0 obj\n{body}\nendobj\n", i + 1));
    }
    let xref = out.len();
    out.push_str(&format!(
        "xref\n0 {}\n0000000000 65535 f \n",
        objects.len() + 1
    ));
    for offset in offsets {
        out.push_str(&format!("{offset:010} 00000 n \n"));
    }
    out.push_str(&format!(
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
        objects.len() + 1
    ));
    out.into_bytes()
}

/// 空白のPDF。
pub fn blank_document() -> Result<PdfDocument, String> {
    open_pdf(&blank_pdf())
}

impl AppWindow {
    /// 表示中のTypstタブのエラー・警告の一覧。無ければ`None`。
    pub(crate) fn render_diagnostics(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        let typst = self.tabs.get(self.active)?.typst.as_ref()?;
        if typst.diagnostics.is_empty() {
            return None;
        }
        let rows = typst.diagnostics.iter().enumerate().map(|(i, d)| {
            let color = match d.severity {
                Level::Error => ERROR_TEXT,
                Level::Warning => WARNING_TEXT,
            };
            let label = match d.severity {
                Level::Error => "error",
                Level::Warning => "warning",
            };
            // 位置が分かるものは、クリックでエディタの該当の位置を開く。
            let target = d
                .path
                .clone()
                .zip(d.line)
                .map(|(path, line)| (path, line, d.column.unwrap_or(1)));
            div()
                .id(("typst-diagnostic", i))
                .flex()
                .flex_col()
                .pb(px(4.0))
                .when_some(target, |row, (path, line, column)| {
                    row.cursor_pointer()
                        .hover(|d| d.bg(rgb(ROW_HOVER)))
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            if let Err(message) = editor::open(&path, line, column) {
                                this.error = Some(message.into());
                                cx.notify();
                            }
                        }))
                })
                .child(
                    div()
                        .text_color(rgb(color))
                        .child(format!("{label}: {}", d.one_line())),
                )
                .children(d.hints.iter().map(|hint| {
                    div()
                        .pl(px(16.0))
                        .text_color(rgb(HINT_TEXT))
                        .child(format!("hint: {hint}"))
                }))
        });
        Some(
            div()
                .id("typst-diagnostics")
                .flex_none()
                .w_full()
                .max_h(px(PANEL_MAX_HEIGHT))
                .overflow_y_scroll()
                .px(px(12.0))
                .py(px(8.0))
                .bg(rgb(PANEL_BACKGROUND))
                .border_t_1()
                .border_color(rgb(PANEL_BORDER))
                .text_sm()
                .children(rows),
        )
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, time::Duration};

    use gpui::{Entity, TestAppContext, VisualTestContext};

    use super::*;
    use crate::reload::POLL_INTERVAL;

    fn project(name: &str, files: &[(&str, &str)]) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("pdf_view_app_typ_{name}_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        for (file, content) in files {
            fs::write(dir.join(file), content).unwrap();
        }
        dir
    }

    /// バックグラウンドの処理が進むまで、時計を進めながら待つ。
    fn settle(cx: &mut VisualTestContext, rounds: usize) {
        cx.executor().allow_parking();
        for _ in 0..rounds {
            cx.executor().advance_clock(POLL_INTERVAL * 2);
            cx.run_until_parked();
            std::thread::sleep(Duration::from_millis(30));
        }
    }

    fn open<'a>(
        path: &Path,
        cx: &'a mut TestAppContext,
    ) -> (Entity<AppWindow>, &'a mut VisualTestContext) {
        let (view, cx) = cx.add_window_view(|window, cx| AppWindow::new(None, None, window, cx));
        view.update_in(cx, |this, window, cx| {
            this.open_path(path.to_path_buf(), window, cx)
        });
        cx.executor().allow_parking();
        for _ in 0..300 {
            cx.run_until_parked();
            if view.read_with(cx, |this, _| !this.tabs.is_empty()) {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        (view, cx)
    }

    fn pages(view: &Entity<AppWindow>, cx: &VisualTestContext) -> usize {
        view.read_with(cx, |this, cx| {
            this.tabs[0].viewer.read(cx).document().page_count()
        })
    }

    fn errors(view: &Entity<AppWindow>, cx: &VisualTestContext) -> usize {
        view.read_with(cx, |this, _| {
            this.tabs[0]
                .typst
                .as_ref()
                .unwrap()
                .diagnostics
                .iter()
                .filter(|d| d.severity == Level::Error)
                .count()
        })
    }

    #[test]
    fn only_typ_files_are_typst() {
        assert!(is_typst(Path::new("a/doc.typ")));
        assert!(is_typst(Path::new("DOC.TYP")));
        assert!(!is_typst(Path::new("doc.pdf")));
        assert!(!is_typst(Path::new("typ")));
    }

    #[test]
    fn the_blank_pdf_is_a_valid_one_page_document() {
        let document = blank_document().unwrap();
        assert_eq!(document.page_count(), 1);
    }

    /// `.typ`を開くと、コンパイルしたPDFがタブに表示される。
    #[gpui::test]
    fn opening_a_typ_file_shows_the_compiled_pdf(cx: &mut TestAppContext) {
        let dir = project("open", &[("main.typ", "= Title\n\nHello")]);
        let (view, cx) = open(&dir.join("main.typ"), cx);
        assert_eq!(view.read_with(cx, |this, _| this.tabs.len()), 1);
        assert_eq!(pages(&view, cx), 1);
        assert_eq!(errors(&view, cx), 0);
    }

    /// 最初のコンパイルが失敗しても、タブは開いてエラーを持つ。表示は空白の1ページ。
    #[gpui::test]
    fn a_broken_document_opens_with_its_errors(cx: &mut TestAppContext) {
        let dir = project("broken", &[("main.typ", "#unknown()")]);
        let (view, cx) = open(&dir.join("main.typ"), cx);
        assert_eq!(view.read_with(cx, |this, _| this.tabs.len()), 1);
        assert_eq!(pages(&view, cx), 1);
        assert!(errors(&view, cx) >= 1);
    }

    /// `.typ`を書き換えて保存すると、自動でコンパイルし直す。
    #[gpui::test]
    fn saving_the_source_recompiles(cx: &mut TestAppContext) {
        let dir = project("edit", &[("main.typ", "page one")]);
        let source = dir.join("main.typ");
        let (view, cx) = open(&source, cx);
        assert_eq!(pages(&view, cx), 1);

        fs::write(&source, "page one\n#pagebreak()\npage two").unwrap();
        settle(cx, 8);
        assert_eq!(pages(&view, cx), 2);
        // コンパイルが終われば、「コンパイル中」の印は消える。
        assert!(view.read_with(cx, |this, _| this.compiling.is_empty()));
    }

    /// `import`したファイルを書き換えても、コンパイルし直す。
    #[gpui::test]
    fn saving_an_imported_file_recompiles(cx: &mut TestAppContext) {
        let dir = project(
            "import",
            &[
                ("main.typ", "#import \"lib.typ\": body\n#body()"),
                ("lib.typ", "#let body() = [one page]"),
            ],
        );
        let (view, cx) = open(&dir.join("main.typ"), cx);
        assert_eq!(pages(&view, cx), 1);

        fs::write(dir.join("lib.typ"), "#let body() = [a #pagebreak() b]").unwrap();
        settle(cx, 8);
        assert_eq!(pages(&view, cx), 2);
    }

    /// エラーを直して保存すると、エラーが消えて、PDFが表示される。壊したままの間は古い表示が残る。
    #[gpui::test]
    fn fixing_an_error_clears_it_and_shows_the_pdf(cx: &mut TestAppContext) {
        let dir = project("fix", &[("main.typ", "ok\n#pagebreak()\nok")]);
        let source = dir.join("main.typ");
        let (view, cx) = open(&source, cx);
        assert_eq!(pages(&view, cx), 2);

        fs::write(&source, "#unknown()").unwrap();
        settle(cx, 8);
        assert!(errors(&view, cx) >= 1, "the error should be shown");
        assert_eq!(pages(&view, cx), 2, "the old view should stay");

        fs::write(&source, "fixed").unwrap();
        settle(cx, 8);
        assert_eq!(errors(&view, cx), 0);
        assert_eq!(pages(&view, cx), 1);
    }

    /// 同じ`.typ`をもう一度開いても、タブは増えない。
    #[gpui::test]
    fn opening_the_same_typ_file_again_does_not_add_a_tab(cx: &mut TestAppContext) {
        let dir = project("again", &[("main.typ", "x")]);
        let source = dir.join("main.typ");
        let (view, cx) = open(&source, cx);
        view.update_in(cx, |this, window, cx| {
            this.open_path(source.clone(), window, cx)
        });
        settle(cx, 3);
        assert_eq!(view.read_with(cx, |this, _| this.tabs.len()), 1);
    }
}
