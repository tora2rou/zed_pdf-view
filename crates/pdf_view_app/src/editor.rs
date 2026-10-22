//! エラーの位置を、外部のエディタで開く。

use std::{
    ffi::OsString,
    path::Path,
    process::{Command, Stdio},
};

/// エディタを指定する環境変数。`{file}` `{line}` `{column}` が置き換わる(例: `zed {file}:{line}:{column}`)。
const EDITOR_ENV: &str = "PDF_VIEW_EDITOR";

/// 既定で試すエディタ。コマンドが使えるものを、上から選ぶ。
const KNOWN_EDITORS: [(&str, &str); 2] = [
    ("zed", "zed {file}:{line}:{column}"),
    ("code", "code --goto {file}:{line}:{column}"),
];

/// 使うエディタのコマンドの雛形。環境変数が優先、なければ`available`が真を返す最初の既知のエディタ。
pub fn template(env: Option<String>, available: impl Fn(&str) -> bool) -> Option<String> {
    if let Some(template) = env.filter(|t| !t.trim().is_empty()) {
        return Some(template);
    }
    KNOWN_EDITORS
        .iter()
        .find(|(program, _)| available(program))
        .map(|(_, template)| (*template).to_string())
}

/// 雛形を、実行するコマンドとその引数にする。空白で区切り、`{file}` などを置き換える(ファイル名に
/// 空白があっても、1つの引数のまま)。
pub fn command(
    template: &str,
    file: &Path,
    line: usize,
    column: usize,
) -> Option<(OsString, Vec<OsString>)> {
    let mut parts = template.split_whitespace().map(|part| {
        let mut out = OsString::new();
        let mut rest = part;
        while let Some(start) = rest.find('{') {
            out.push(&rest[..start]);
            let after = &rest[start..];
            if let Some(tail) = after.strip_prefix("{file}") {
                out.push(file);
                rest = tail;
            } else if let Some(tail) = after.strip_prefix("{line}") {
                out.push(line.to_string());
                rest = tail;
            } else if let Some(tail) = after.strip_prefix("{column}") {
                out.push(column.to_string());
                rest = tail;
            } else {
                out.push("{");
                rest = &after[1..];
            }
        }
        out.push(rest);
        out
    });
    let program = parts.next()?;
    Some((program, parts.collect()))
}

/// コマンドが起動できるか。
fn runs(program: &str) -> bool {
    Command::new(program)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok()
}

/// `file`の`line`行`column`列をエディタで開く。エディタが見つからない、起動できないときは理由を返す。
pub fn open(file: &Path, line: usize, column: usize) -> Result<(), String> {
    open_with(std::env::var(EDITOR_ENV).ok(), file, line, column)
}

/// `open`の、環境変数の値(`env`)を引数で受ける版。
fn open_with(env: Option<String>, file: &Path, line: usize, column: usize) -> Result<(), String> {
    let template = template(env, runs).ok_or_else(|| {
        format!("エディタが見つかりません。環境変数{EDITOR_ENV}にコマンドを指定してください(例: zed {{file}}:{{line}}:{{column}})")
    })?;
    let (program, args) = command(&template, file, line, column)
        .ok_or_else(|| format!("{EDITOR_ENV}のコマンドが空です"))?;
    let mut command = Command::new(&program);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        // `code`などはコンソールのコマンドなので、窓が出ないようにする。
        command.creation_flags(0x0800_0000);
    }
    command
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("{}を起動できません: {e}", program.to_string_lossy()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_environment_variable_wins() {
        assert_eq!(
            template(Some("nvim +{line} {file}".into()), |_| true).as_deref(),
            Some("nvim +{line} {file}")
        );
    }

    #[test]
    fn a_blank_variable_is_ignored() {
        let t = template(Some("  ".into()), |p| p == "code");
        assert_eq!(t.as_deref(), Some("code --goto {file}:{line}:{column}"));
    }

    #[test]
    fn known_editors_are_tried_in_order() {
        assert_eq!(
            template(None, |_| true).as_deref(),
            Some("zed {file}:{line}:{column}")
        );
        assert_eq!(
            template(None, |p| p == "code").as_deref(),
            Some("code --goto {file}:{line}:{column}")
        );
        assert_eq!(template(None, |_| false), None);
    }

    #[test]
    fn placeholders_are_replaced() {
        let (program, args) =
            command("zed {file}:{line}:{column}", Path::new("/a/b.typ"), 4, 7).unwrap();
        assert_eq!(program, "zed");
        assert_eq!(args, ["/a/b.typ:4:7"]);
    }

    /// ファイル名に空白があっても、1つの引数のまま渡る。
    #[test]
    fn a_path_with_spaces_stays_one_argument() {
        let (_, args) = command("ed +{line} {file}", Path::new("/my docs/a b.typ"), 12, 1).unwrap();
        assert_eq!(args, ["+12", "/my docs/a b.typ"]);
    }

    #[test]
    fn unknown_braces_are_left_alone() {
        let (_, args) = command("run {nope} {file}", Path::new("f"), 1, 1).unwrap();
        assert_eq!(args, ["{nope}", "f"]);
    }

    #[test]
    fn an_empty_template_has_no_command() {
        assert!(command("   ", Path::new("f"), 1, 1).is_none());
    }

    /// 雛形のコマンドが実際に起動される(ここでは`touch`で、起動したことを確かめる)。
    #[cfg(unix)]
    #[test]
    fn the_command_is_actually_launched() {
        let dir = std::env::temp_dir().join(format!("pdf_view_editor_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("doc.typ");
        open_with(Some("touch {file}.opened".into()), &file, 3, 5).unwrap();
        let marker = dir.join("doc.typ.opened");
        for _ in 0..200 {
            if marker.exists() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("the editor command was not run");
    }

    #[test]
    fn a_missing_editor_is_reported() {
        let error = open_with(
            Some("pdf-view-no-such-editor {file}".into()),
            Path::new("f"),
            1,
            1,
        )
        .unwrap_err();
        assert!(error.contains("起動できません"), "{error}");
    }
}
