//! LaTeX文書をPDFにコンパイルする(外部コマンドの`latexmk`を使う)。GUIには依存しない。

mod engine;
mod fls;
mod log;

use std::{
    collections::hash_map::DefaultHasher,
    ffi::{OsStr, OsString},
    fs,
    hash::{Hash, Hasher},
    io::ErrorKind,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use build_diag::{Diagnostic, Level};

pub use engine::{Engine, detect};

/// 外部コマンドとして呼ぶ`latexmk`の名前。
const LATEXMK: &str = "latexmk";
/// `latexmk`を指定する環境変数(PATHに無い場所のものを使うとき)。
const LATEXMK_ENV: &str = "PDF_VIEW_LATEXMK";
/// ログが読めない失敗のときに、メッセージに付ける出力の末尾の行数。
const TAIL_LINES: usize = 12;
/// Windowsで、子プロセスにコンソールの窓を出さないためのフラグ(`CREATE_NO_WINDOW`)。
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

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

/// `path`のLaTeX文書をPDFにコンパイルする。補助ファイル(`.aux`など)は、文書の隣ではなく一時フォルダに
/// 置く。同じ文書を続けてコンパイルするときは、その補助ファイルを使い回すので、2回目以降は速い。
pub fn compile_file(path: &Path) -> Result<Compiled, Failure> {
    let fail = |message: String, dependencies: Vec<PathBuf>| Failure {
        errors: vec![Diagnostic {
            file: Some(path.display().to_string()),
            path: Some(path.to_path_buf()),
            ..Diagnostic::message(Level::Error, message)
        }],
        warnings: Vec::new(),
        dependencies,
    };
    let main = path.canonicalize().map_err(|e| {
        fail(
            format!("ファイルを開けません: {e}"),
            vec![path.to_path_buf()],
        )
    })?;
    let project = main
        .parent()
        .ok_or_else(|| {
            fail(
                "ファイルの置かれたフォルダが分かりません".into(),
                vec![main.clone()],
            )
        })?
        .to_path_buf();
    let source = fs::read_to_string(&main)
        .map_err(|e| fail(format!("ファイルを読めません: {e}"), vec![main.clone()]))?;
    let engine = detect(&source);
    let outdir = output_dir(&main);
    fs::create_dir_all(&outdir)
        .map_err(|e| fail(format!("出力先を作れません: {e}"), vec![main.clone()]))?;

    let output = run(&latexmk_program(), engine, &main, &project, &outdir)
        .map_err(|m| fail(m, vec![main.clone()]))?;

    let stem = main
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let dependencies = dependencies(&outdir, &stem, &project, &main);
    let log_text = fs::read(outdir.join(format!("{stem}.log")))
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default();
    let diagnostics = log::parse(&log_text, &project, &main);
    let (mut errors, warnings): (Vec<_>, Vec<_>) = diagnostics
        .into_iter()
        .partition(|d| d.severity == Level::Error);

    let pdf = fs::read(outdir.join(format!("{stem}.pdf")));
    match pdf {
        Ok(pdf) if output.success => Ok(Compiled {
            pdf,
            warnings,
            dependencies,
        }),
        _ => {
            if errors.is_empty() {
                // ログから原因が分からなかったとき(latexmkの設定の誤りなど)は、出力の末尾を見せる。
                errors.push(Diagnostic {
                    file: Some(path.display().to_string()),
                    path: Some(main.clone()),
                    ..Diagnostic::message(Level::Error, tail(&output.text))
                });
            }
            Err(Failure {
                errors,
                warnings,
                dependencies,
            })
        }
    }
}

/// `latexmk`の実行結果。
struct Output {
    success: bool,
    /// 標準出力と標準エラー出力。
    text: String,
}

/// 補助ファイルの置き場所。文書のパスごとに別のフォルダにする(プロセスごと)。
fn output_dir(main: &Path) -> PathBuf {
    let mut hasher = DefaultHasher::new();
    main.hash(&mut hasher);
    std::env::temp_dir()
        .join(format!("chihayaburu-latex-{}", std::process::id()))
        .join(format!("{:016x}", hasher.finish()))
}

fn latexmk_program() -> OsString {
    std::env::var_os(LATEXMK_ENV)
        .filter(|p| !p.is_empty())
        .unwrap_or_else(|| LATEXMK.into())
}

fn run(
    program: &OsStr,
    engine: Engine,
    main: &Path,
    project: &Path,
    outdir: &Path,
) -> Result<Output, String> {
    let mut command = Command::new(program);
    command
        .current_dir(project)
        .args(engine.latexmk_args())
        .args([
            // エラーで止まって入力待ちにならないように、対話なし。
            "-interaction=nonstopmode",
            // エラーを`ファイル:行: メッセージ`の形にする。
            "-file-line-error",
            // 読み込んだファイルを記録させる(依存ファイルの一覧になる)。
            "-recorder",
        ])
        .arg({
            // `-outdir=パス`の1つの引数にする(`-outdir`と`パス`に分けると、latexmkに拒否される)。
            let mut arg = OsString::from("-outdir=");
            arg.push(outdir);
            arg
        })
        // 文書は、フォルダの中の名前で渡す(絶対パスだと、ログの行が長くなって折り返される)。
        .arg(main.file_name().unwrap_or(main.as_os_str()))
        // ログの行を折り返さない(折り返されると、長いメッセージが途中で切れる)。
        .env("max_print_line", "10000")
        .env("error_line", "254")
        .env("half_error_line", "238")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let output = command.output().map_err(|e| {
        if e.kind() == ErrorKind::NotFound {
            format!(
                "{}コマンドが見つかりません。TeX Live(latexmk)をインストールするか、{LATEXMK_ENV}に場所を指定してください",
                program.to_string_lossy()
            )
        } else {
            format!("{}を起動できません: {e}", program.to_string_lossy())
        }
    })?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(Output {
        success: output.status.success(),
        text,
    })
}

/// 読み込んだプロジェクト内のファイル。記録(`.fls`)が読めなければ、文書本体だけ。
fn dependencies(outdir: &Path, stem: &str, project: &Path, main: &Path) -> Vec<PathBuf> {
    let mut files = fs::read_to_string(outdir.join(format!("{stem}.fls")))
        .map(|fls| fls::project_inputs(&fls, project, outdir))
        .unwrap_or_default();
    if !files.iter().any(|f| f == main) {
        files.push(main.to_path_buf());
        files.sort();
    }
    files
}

/// 出力の末尾の数行。
fn tail(text: &str) -> String {
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    let start = lines.len().saturating_sub(TAIL_LINES);
    let tail = lines[start..].join("\n");
    if tail.is_empty() {
        "latexmkが失敗しました".to_string()
    } else {
        format!("latexmkが失敗しました:\n{tail}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn latexmk_available() -> bool {
        Command::new(LATEXMK)
            .arg("-v")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok()
    }

    fn project(name: &str, files: &[(&str, &str)]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("latex_build_{name}_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        for (file, content) in files {
            let path = dir.join(file);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, content).unwrap();
        }
        dir
    }

    const HELLO: &str =
        "\\documentclass{article}\n\\begin{document}\nHello, world.\n\\end{document}\n";

    #[test]
    fn a_valid_document_becomes_a_pdf() {
        if !latexmk_available() {
            return;
        }
        let dir = project("valid", &[("main.tex", HELLO)]);
        let compiled = compile_file(&dir.join("main.tex")).expect("should compile");
        assert!(compiled.pdf.starts_with(b"%PDF-"));
        assert!(
            compiled
                .dependencies
                .iter()
                .any(|p| p.ends_with("main.tex"))
        );
        // 補助ファイルは、文書の隣には作らない。
        assert!(!dir.join("main.aux").exists());
        assert!(!dir.join("main.log").exists());
    }

    #[test]
    fn errors_have_a_file_and_line() {
        if !latexmk_available() {
            return;
        }
        let src =
            "\\documentclass{article}\n\\begin{document}\nHello \\foo world.\n\\end{document}\n";
        let dir = project("error", &[("main.tex", src)]);
        let failure = compile_file(&dir.join("main.tex")).unwrap_err();
        let first = &failure.errors[0];
        assert_eq!(first.message, "Undefined control sequence.");
        assert_eq!(first.file.as_deref(), Some("main.tex"));
        assert_eq!(first.line, Some(3));
    }

    /// `\input`した別ファイルのエラーは、そのファイルを指し、そのファイルも依存に入る。
    #[test]
    fn included_files_are_dependencies_and_report_their_errors() {
        if !latexmk_available() {
            return;
        }
        let main =
            "\\documentclass{article}\n\\begin{document}\n\\input{chapters/one}\n\\end{document}\n";
        let dir = project(
            "input",
            &[("main.tex", main), ("chapters/one.tex", "fine\n\\bar\n")],
        );
        let failure = compile_file(&dir.join("main.tex")).unwrap_err();
        assert!(
            failure
                .errors
                .iter()
                .any(|e| e.file.as_deref() == Some("chapters/one.tex") && e.line == Some(2)),
            "{:?}",
            failure.errors
        );
        assert!(
            failure
                .dependencies
                .iter()
                .any(|p| p.ends_with("chapters/one.tex"))
        );
    }

    /// 直したら、前のエラーが残らずPDFができる(補助ファイルを使い回しても)。
    #[test]
    fn fixing_the_error_gives_a_pdf() {
        if !latexmk_available() {
            return;
        }
        let dir = project(
            "fix",
            &[(
                "main.tex",
                "\\documentclass{article}\n\\begin{document}\n\\oops\n\\end{document}\n",
            )],
        );
        let path = dir.join("main.tex");
        assert!(compile_file(&path).is_err());
        fs::write(&path, HELLO).unwrap();
        assert!(compile_file(&path).is_ok());
    }

    #[test]
    fn a_japanese_document_compiles_with_lualatex() {
        if !latexmk_available() {
            return;
        }
        let src = "% !TEX program = lualatex\n\\documentclass{ltjsarticle}\n\\begin{document}\n日本語の本文です。\n\\end{document}\n";
        let dir = project("ja", &[("main.tex", src)]);
        let result = compile_file(&dir.join("main.tex"));
        assert!(result.is_ok(), "{:?}", result.err().map(|f| f.errors));
    }

    #[test]
    fn a_japanese_document_compiles_with_platex() {
        if !latexmk_available() {
            return;
        }
        let src =
            "\\documentclass{jsarticle}\n\\begin{document}\n日本語の本文です。\n\\end{document}\n";
        let dir = project("ptex", &[("main.tex", src)]);
        let result = compile_file(&dir.join("main.tex"));
        assert!(result.is_ok(), "{:?}", result.err().map(|f| f.errors));
    }

    #[test]
    fn a_missing_source_is_an_error_not_a_panic() {
        let failure = compile_file(Path::new("/nonexistent/main.tex")).unwrap_err();
        assert!(failure.errors[0].message.contains("開けません"));
    }

    #[test]
    fn a_missing_latexmk_is_reported_clearly() {
        let dir = project("nolatexmk", &[("main.tex", HELLO)]);
        let message = run(
            OsStr::new("pdf-view-no-such-latexmk"),
            Engine::Pdflatex,
            &dir.join("main.tex"),
            &dir,
            &dir.join("out"),
        )
        .err()
        .expect("should fail");
        assert!(message.contains("見つかりません"), "{message}");
    }

    #[test]
    fn the_tail_keeps_the_last_lines() {
        let text = (1..=30).map(|n| format!("line {n}\n")).collect::<String>();
        let t = tail(&text);
        assert!(t.contains("line 30") && !t.contains("line 1\n"));
        assert_eq!(tail(""), "latexmkが失敗しました");
    }

    fn sample(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../samples/latex")
            .join(name)
    }

    /// サンプルは、実際にコンパイルできる(壊れていると、試すときに困る)。
    #[test]
    fn the_samples_compile() {
        if !latexmk_available() {
            return;
        }
        for name in ["basic.tex", "japanese.tex", "with-input/main.tex"] {
            let result = compile_file(&sample(name));
            assert!(
                result.is_ok(),
                "{name}: {:?}",
                result.err().map(|f| f.errors)
            );
        }
    }

    /// エラーのサンプルは、期待どおりの位置でエラーを返す。
    #[test]
    fn the_broken_sample_reports_its_errors() {
        if !latexmk_available() {
            return;
        }
        let failure = compile_file(&sample("broken.tex")).unwrap_err();
        let first = &failure.errors[0];
        assert_eq!(first.message, "Undefined control sequence.");
        assert_eq!(first.line, Some(8));
    }
}
