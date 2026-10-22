//! Typstの文書をPDFにコンパイルする。GUIには依存しない。

mod world;

use std::path::{Path, PathBuf};

use typst::{
    World as _, WorldExt as _,
    diag::{Severity, SourceDiagnostic},
};
use typst_layout::PagedDocument;
use typst_pdf::PdfOptions;

pub use build_diag::{Diagnostic, Level};

use world::FileWorld;

/// コンパイルに成功した結果。
#[derive(Debug)]
pub struct Compiled {
    pub pdf: Vec<u8>,
    pub warnings: Vec<Diagnostic>,
    /// 読み込んだプロジェクト内のファイル。これらが書き換わったら、コンパイルし直す。
    pub dependencies: Vec<PathBuf>,
}

/// コンパイルに失敗した結果。
#[derive(Debug)]
pub struct Failure {
    pub errors: Vec<Diagnostic>,
    pub warnings: Vec<Diagnostic>,
    /// 読み込んだプロジェクト内のファイル。直したら、コンパイルし直す。
    pub dependencies: Vec<PathBuf>,
}

/// `path`のTypst文書をPDFにコンパイルする。プロジェクトのルートは`path`のあるフォルダ。
pub fn compile_file(path: &Path) -> Result<Compiled, Failure> {
    let mut world = FileWorld::new(path).map_err(|message| Failure {
        errors: vec![Diagnostic {
            severity: Level::Error,
            message,
            file: Some(path.display().to_string()),
            path: Some(path.to_path_buf()),
            line: None,
            column: None,
            hints: Vec::new(),
        }],
        warnings: Vec::new(),
        dependencies: vec![path.to_path_buf()],
    })?;
    let typst::diag::Warned { output, warnings } = typst::compile::<PagedDocument>(&world);
    let warnings = diagnostics(&world, &warnings);
    let result = output.and_then(|document| typst_pdf::pdf(&document, &PdfOptions::default()));
    match result {
        Ok(pdf) => Ok(Compiled {
            pdf,
            warnings,
            dependencies: world.dependencies(),
        }),
        Err(errors) => Err(Failure {
            errors: diagnostics(&world, &errors),
            warnings,
            dependencies: world.dependencies(),
        }),
    }
}

fn diagnostics<'a>(
    world: &FileWorld,
    items: impl IntoIterator<Item = &'a SourceDiagnostic>,
) -> Vec<Diagnostic> {
    items.into_iter().map(|d| diagnostic(world, d)).collect()
}

fn diagnostic(world: &FileWorld, diagnostic: &SourceDiagnostic) -> Diagnostic {
    let location = locate(world, diagnostic);
    Diagnostic {
        severity: match diagnostic.severity {
            Severity::Error => Level::Error,
            Severity::Warning => Level::Warning,
        },
        message: diagnostic.message.to_string(),
        file: location.as_ref().map(|l| l.file.clone()),
        path: location.as_ref().and_then(|l| l.path.clone()),
        line: location.as_ref().map(|l| l.line),
        column: location.as_ref().map(|l| l.column),
        hints: diagnostic
            .hints
            .iter()
            .map(|hint| hint.v.to_string())
            .collect(),
    }
}

struct Location {
    file: String,
    path: Option<PathBuf>,
    line: usize,
    column: usize,
}

/// エラーの位置を、ファイル名と行・列にする。位置が無い(文書全体の)ものは`None`。
fn locate(world: &FileWorld, diagnostic: &SourceDiagnostic) -> Option<Location> {
    let id = diagnostic.span.id()?;
    let source = world.source(id).ok()?;
    let range = world.range(diagnostic.span)?;
    let (line, column) = source.lines().byte_to_line_column(range.start)?;
    Some(Location {
        file: world.display_name(id),
        path: world.real_path(id),
        line: line + 1,
        column: column + 1,
    })
}

/// 出力するPDFの置き場所。入力の拡張子を`.pdf`にしたもの。
pub fn pdf_path_for(source: &Path) -> PathBuf {
    source.with_extension("pdf")
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    /// テスト用の一時フォルダに`files`(名前と中身)を書き、フォルダを返す。
    fn project(name: &str, files: &[(&str, &str)]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("typst_build_{name}_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        for (file, content) in files {
            fs::write(dir.join(file), content).unwrap();
        }
        dir
    }

    #[test]
    fn a_valid_document_becomes_a_pdf() {
        let dir = project("valid", &[("main.typ", "= Title\n\nHello, *world*!")]);
        let compiled = compile_file(&dir.join("main.typ")).expect("should compile");
        assert!(compiled.pdf.starts_with(b"%PDF-"), "not a PDF");
        assert!(compiled.warnings.is_empty(), "{:?}", compiled.warnings);
    }

    /// 日本語も組める(文書の中の文字は、システムか同梱のフォントで描かれる)。
    #[test]
    fn japanese_text_compiles() {
        let dir = project(
            "japanese",
            &[("main.typ", "= 見出し\n\n日本語の本文です。")],
        );
        assert!(compile_file(&dir.join("main.typ")).is_ok());
    }

    /// 他のファイルを読み込める。
    #[test]
    fn included_files_are_loaded() {
        let dir = project(
            "include",
            &[
                ("main.typ", "#import \"lib.typ\": greet\n#greet(\"x\")"),
                ("lib.typ", "#let greet(name) = [Hi #name]"),
            ],
        );
        assert!(compile_file(&dir.join("main.typ")).is_ok());
    }

    /// エラーはファイル名・行・列つきで返る。
    #[test]
    fn errors_have_a_location() {
        let dir = project(
            "error",
            &[("main.typ", "= Title\n\n#let x = 1\n#unknown_fn(x)\n")],
        );
        let failure = compile_file(&dir.join("main.typ")).unwrap_err();
        let first = &failure.errors[0];
        assert_eq!(first.severity, Level::Error);
        assert_eq!(first.file.as_deref(), Some("main.typ"));
        assert_eq!(first.line, Some(4), "{first:?}");
        assert_eq!(
            first.path.as_deref(),
            Some(dir.canonicalize().unwrap().join("main.typ").as_path())
        );
        assert!(
            first.one_line().starts_with("main.typ:4:"),
            "{}",
            first.one_line()
        );
    }

    /// 読み込むファイル側のエラーは、そのファイルの位置を指す。
    #[test]
    fn errors_in_an_included_file_point_to_that_file() {
        let dir = project(
            "error_in_lib",
            &[
                ("main.typ", "#import \"lib.typ\": f\n#f()"),
                ("lib.typ", "#let f() = {\n  nope\n}"),
            ],
        );
        let failure = compile_file(&dir.join("main.typ")).unwrap_err();
        assert!(
            failure
                .errors
                .iter()
                .any(|e| e.file.as_deref() == Some("lib.typ") && e.line == Some(2)),
            "{:?}",
            failure.errors
        );
    }

    /// 読み込んだファイルが依存として返る(メインを含み、読み込んでいないファイルは含まない)。
    #[test]
    fn dependencies_list_the_files_that_were_read() {
        let dir = project(
            "deps",
            &[
                ("main.typ", "#import \"lib.typ\": f\n#f()"),
                ("lib.typ", "#let f() = [x]"),
                ("unused.typ", "= not used"),
            ],
        );
        let compiled = compile_file(&dir.join("main.typ")).unwrap();
        let names: Vec<_> = compiled
            .dependencies
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["lib.typ", "main.typ"]);
    }

    /// 失敗したときも依存が返る(直したらコンパイルし直せるように)。
    #[test]
    fn a_failed_compile_still_reports_its_dependencies() {
        let dir = project("deps_fail", &[("main.typ", "#unknown()")]);
        let failure = compile_file(&dir.join("main.typ")).unwrap_err();
        assert_eq!(failure.dependencies.len(), 1);
    }

    #[test]
    fn a_missing_file_is_an_error_not_a_panic() {
        let failure = compile_file(Path::new("/nonexistent/dir/main.typ")).unwrap_err();
        assert_eq!(failure.errors.len(), 1);
        assert!(failure.errors[0].message.contains("開けません"));
    }

    #[test]
    fn the_pdf_goes_next_to_the_source() {
        assert_eq!(
            pdf_path_for(Path::new("/a/b/doc.typ")),
            Path::new("/a/b/doc.pdf")
        );
    }

    fn sample(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../samples/typst")
            .join(name)
    }

    /// サンプルは、実際にコンパイルできる(壊れていると、試すときに困る)。
    #[test]
    fn the_samples_compile() {
        for name in ["basic.typ", "long.typ", "with-import/main.typ"] {
            let result = compile_file(&sample(name));
            assert!(
                result.is_ok(),
                "{name}: {:?}",
                result.err().map(|f| f.errors)
            );
        }
    }

    /// 評価エラーのサンプルは、最初の1件(11行目)だけが、位置つきで返る。
    #[test]
    fn the_broken_sample_reports_its_first_error() {
        let failure = compile_file(&sample("broken.typ")).unwrap_err();
        assert_eq!(failure.errors.len(), 1, "{:?}", failure.errors);
        assert_eq!(failure.errors[0].line, Some(11));
        assert!(failure.errors[0].message.contains("cannot add"));
    }

    /// 構文エラーのサンプルは、複数のエラーと、ヒントを返す。
    #[test]
    fn the_syntax_sample_reports_several_errors_with_hints() {
        let failure = compile_file(&sample("broken-syntax.typ")).unwrap_err();
        assert!(failure.errors.len() >= 3, "{:?}", failure.errors);
        assert!(failure.errors.iter().all(|e| e.line.is_some()));
        assert!(failure.errors.iter().any(|e| !e.hints.is_empty()));
    }
}
