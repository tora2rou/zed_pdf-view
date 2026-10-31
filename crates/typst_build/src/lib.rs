//! Typstの文書をPDFにコンパイルする。GUIには依存しない。

mod world;

use std::{
    collections::HashMap,
    num::NonZeroUsize,
    path::{Path, PathBuf},
};

use typst::{
    Library, World, WorldExt as _,
    diag::{FileResult, Severity, SourceDiagnostic},
    foundations::{Bytes, Datetime, Duration},
    introspection::PagedPosition,
    layout::{Abs, Point},
    syntax::{FileId, Source},
    text::{Font, FontBook},
    utils::LazyHash,
};
use typst_ide::Jump;
use typst_layout::PagedDocument;
use typst_pdf::PdfOptions;

use world::FileWorld;

/// エラーまたは警告1件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Level,
    pub message: String,
    /// 該当するファイル。プロジェクトの外(パッケージなど)のファイルは、仮想パスのまま。
    pub file: Option<String>,
    /// 該当するファイルの、ファイルシステム上の場所(エディタで開くための絶対パス)。
    pub path: Option<PathBuf>,
    /// 該当する位置(1始まりの行・列)。
    pub line: Option<usize>,
    pub column: Option<usize>,
    pub hints: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Error,
    Warning,
}

impl Diagnostic {
    /// `file:line:column: message`の1行の形。
    pub fn one_line(&self) -> String {
        let mut out = String::new();
        if let Some(file) = &self.file {
            out.push_str(file);
            if let (Some(line), Some(column)) = (self.line, self.column) {
                out.push_str(&format!(":{line}:{column}"));
            }
            out.push_str(": ");
        }
        out.push_str(&self.message);
        out
    }
}

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

/// コンパイルのキャッシュのうち、直近この回数のコンパイルで使われなかったものは捨てる。
/// 編集のたびにコンパイルするので、捨てないとメモリが増え続ける。
const CACHE_MAX_AGE: usize = 10;

/// `path`のTypst文書をPDFにコンパイルする。プロジェクトのルートは`path`のあるフォルダ。
pub fn compile_file(path: &Path) -> Result<Compiled, Failure> {
    let mut world = FileWorld::new(path).map_err(|message| setup_failure(path, message))?;
    compile_world(&mut world).map(|(compiled, _)| compiled)
}

fn setup_failure(path: &Path, message: String) -> Failure {
    Failure {
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
    }
}

fn compile_world(world: &mut FileWorld) -> Result<(Compiled, PagedDocument), Failure> {
    let typst::diag::Warned { output, warnings } = typst::compile::<PagedDocument>(&*world);
    let warnings = diagnostics(world, &warnings);
    let result = output.and_then(|document| {
        let pdf = typst_pdf::pdf(&document, &PdfOptions::default())?;
        Ok((pdf, document))
    });
    comemo::evict(CACHE_MAX_AGE);
    match result {
        Ok((pdf, document)) => Ok((
            Compiled {
                pdf,
                warnings,
                dependencies: world.dependencies(),
            },
            document,
        )),
        Err(errors) => Err(Failure {
            errors: diagnostics(world, &errors),
            warnings,
            dependencies: world.dependencies(),
        }),
    }
}

/// ソースの中の位置(1始まりの行・列)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourcePosition {
    pub path: PathBuf,
    pub line: usize,
    pub column: usize,
}

/// 1つの文書を、編集に合わせて何度もコンパイルする。前回の結果を使い回すので、2回目以降は速い。
/// 最後に成功した結果の文書を持ち、プレビューのクリック位置からソースの位置を引ける。
pub struct Session {
    world: FileWorld,
    /// 最後に成功した結果の文書と、それを作ったときのソース。
    document: Option<(PagedDocument, SourceSnapshot)>,
}

impl Session {
    /// `main`をメインのファイルとするセッション。プロジェクトのルートは`main`のあるフォルダ。
    pub fn new(main: &Path) -> Result<Self, Failure> {
        let world = FileWorld::new(main).map_err(|message| setup_failure(main, message))?;
        Ok(Self {
            world,
            document: None,
        })
    }

    /// コンパイルする。`overlays`(ファイルの場所 → エディタで編集中の内容)は、ディスクより優先する。
    /// 失敗したときは、前回成功した文書をそのまま残す(プレビューは古い表示を見せ続けるため)。
    pub fn compile(&mut self, overlays: HashMap<PathBuf, String>) -> Result<Compiled, Failure> {
        self.world.prepare(overlays);
        let (compiled, document) = compile_world(&mut self.world)?;
        let sources = SourceSnapshot {
            sources: self.world.sources_used(),
        };
        self.document = Some((document, sources));
        Ok(compiled)
    }

    /// 最後に成功した結果の`page`ページ目(0始まり)の、左上から(`x`, `y`)ポイントの位置を作った
    /// ソースの位置。文字などがなければ`None`。
    pub fn source_at(&self, page: usize, x: f32, y: f32) -> Option<SourcePosition> {
        let (document, sources) = self.document.as_ref()?;
        let position = PagedPosition {
            page: NonZeroUsize::new(page + 1)?,
            point: Point::new(Abs::pt(f64::from(x)), Abs::pt(f64::from(y))),
        };
        // 文書の中の位置は、作ったときのソースを指している。その後に編集されていても正しく引けるよう、
        // そのときのソースで調べる。
        let world = SnapshotWorld {
            world: &self.world,
            sources,
        };
        let Jump::File(id, offset) = typst_ide::jump_from_click(&world, document, &position)?
        else {
            return None;
        };
        let source = world.source(id).ok()?;
        let (line, column) = source.lines().byte_to_line_column(offset)?;
        Some(SourcePosition {
            path: self.world.real_path(id)?,
            line: line + 1,
            column: column + 1,
        })
    }
}

/// コンパイルに使ったソース。ソースは書き換えるときに複製されるので、持っていても編集中のものとは別。
struct SourceSnapshot {
    sources: HashMap<FileId, Source>,
}

/// ソースだけを`SourceSnapshot`から返す`World`。それ以外は`world`に任せる。
struct SnapshotWorld<'a> {
    world: &'a FileWorld,
    sources: &'a SourceSnapshot,
}

impl typst_ide::IdeWorld for SnapshotWorld<'_> {
    fn upcast(&self) -> &dyn World {
        self
    }
}

impl World for SnapshotWorld<'_> {
    fn library(&self) -> &LazyHash<Library> {
        self.world.library()
    }

    fn book(&self) -> &LazyHash<FontBook> {
        self.world.book()
    }

    fn main(&self) -> FileId {
        self.world.main()
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        match self.sources.sources.get(&id) {
            Some(source) => Ok(source.clone()),
            None => self.world.source(id),
        }
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        self.world.file(id)
    }

    fn font(&self, index: usize) -> Option<Font> {
        self.world.font(index)
    }

    fn today(&self, offset: Option<Duration>) -> Option<Datetime> {
        self.world.today(offset)
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

    /// 編集中の内容(未保存)でコンパイルできる。ディスクの内容は使わない。
    #[test]
    fn a_session_compiles_unsaved_text() {
        let dir = project("session_overlay", &[("main.typ", "#unknown()")]);
        let main = dir.join("main.typ");
        let mut session = Session::new(&main).unwrap();
        assert!(session.compile(HashMap::new()).is_err());
        let overlays = HashMap::from([(main.clone(), "= Fixed".to_string())]);
        assert!(session.compile(overlays).is_ok());
        // 編集中の内容を渡さなければ、ディスクの内容に戻る。
        assert!(session.compile(HashMap::new()).is_err());
    }

    /// 読み込まれる側のファイルも、編集中の内容が使われる。
    #[test]
    fn a_session_uses_unsaved_text_of_imported_files() {
        let dir = project(
            "session_import",
            &[
                ("main.typ", "#import \"lib.typ\": f\n#f()"),
                ("lib.typ", "#let f() = nope"),
            ],
        );
        let mut session = Session::new(&dir.join("main.typ")).unwrap();
        assert!(session.compile(HashMap::new()).is_err());
        let overlays = HashMap::from([(dir.join("lib.typ"), "#let f() = [ok]".to_string())]);
        assert!(session.compile(overlays).is_ok());
    }

    /// プレビューのクリック位置から、その文字を書いたソースの行が分かる。
    #[test]
    fn source_at_maps_a_click_to_the_line() {
        let dir = project(
            "session_jump",
            &[(
                "main.typ",
                "#set page(width: 200pt, height: 200pt, margin: 10pt)\n#set text(size: 20pt)\nFirst\n\nSecond\n",
            )],
        );
        let mut session = Session::new(&dir.join("main.typ")).unwrap();
        assert_eq!(
            session.source_at(0, 15.0, 20.0),
            None,
            "nothing compiled yet"
        );
        session.compile(HashMap::new()).unwrap();

        let first = session.source_at(0, 15.0, 20.0).expect("first line");
        assert_eq!(first.line, 3);
        assert_eq!(first.path, dir.canonicalize().unwrap().join("main.typ"));
        let second = session.source_at(0, 15.0, 55.0).expect("second paragraph");
        assert_eq!(second.line, 5);
        // 余白(何も描かれていない所)や、無いページは`None`。
        assert_eq!(session.source_at(0, 190.0, 190.0), None);
        assert_eq!(session.source_at(5, 15.0, 20.0), None);
    }

    /// 失敗しても、前回成功した文書でクリック位置を引ける(表示は古いまま残るので)。
    #[test]
    fn source_at_keeps_working_after_a_failed_compile() {
        let dir = project(
            "session_jump_fail",
            &[(
                "main.typ",
                "#set page(width: 200pt, height: 200pt, margin: 10pt)\n#set text(size: 20pt)\nFirst\n",
            )],
        );
        let main = dir.join("main.typ");
        let mut session = Session::new(&main).unwrap();
        session.compile(HashMap::new()).unwrap();
        let broken = HashMap::from([(main, "#unknown()".to_string())]);
        assert!(session.compile(broken).is_err());
        assert_eq!(session.source_at(0, 15.0, 20.0).map(|p| p.line), Some(3));
    }
}
