//! `latexmk -recorder`が出す`.fls`から、読み込んだファイルの一覧を取る。

use std::path::{Path, PathBuf};

/// 読み込んだファイルのうち、`project`(文書のあるフォルダ)の中にあるもの。`ignore`(出力先)の中と、
/// 存在しないファイルは除く。順番は重複なしで、パス順。
pub fn project_inputs(fls: &str, project: &Path, ignore: &Path) -> Vec<PathBuf> {
    let mut pwd = project.to_path_buf();
    let mut inputs = Vec::new();
    for line in fls.lines() {
        if let Some(dir) = line.strip_prefix("PWD ") {
            pwd = PathBuf::from(dir.trim());
        } else if let Some(file) = line.strip_prefix("INPUT ") {
            let path = pwd.join(file.trim());
            let path = normalize(&path);
            if path.starts_with(project) && !path.starts_with(ignore) && path.is_file() {
                inputs.push(path);
            }
        }
    }
    inputs.sort();
    inputs.dedup();
    inputs
}

/// `.`と`..`を字面で畳む(ファイルシステムには触らない)。
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn only_existing_project_files_are_kept() {
        let dir = std::env::temp_dir().join(format!("latex_fls_{}", std::process::id()));
        fs::create_dir_all(dir.join("sub")).unwrap();
        fs::create_dir_all(dir.join("out")).unwrap();
        fs::write(dir.join("main.tex"), "").unwrap();
        fs::write(dir.join("sub/ch1.tex"), "").unwrap();
        fs::write(dir.join("out/main.aux"), "").unwrap();
        let fls = format!(
            "PWD {pwd}\nINPUT /usr/share/texlive/article.cls\nINPUT main.tex\nINPUT ./sub/ch1.tex\nINPUT sub/../sub/ch1.tex\nINPUT out/main.aux\nINPUT missing.tex\nOUTPUT out/main.log\n",
            pwd = dir.display()
        );
        let inputs = project_inputs(&fls, &dir, &dir.join("out"));
        assert_eq!(inputs, [dir.join("main.tex"), dir.join("sub/ch1.tex")]);
    }

    #[test]
    fn an_empty_record_has_no_inputs() {
        assert!(project_inputs("", Path::new("/nope"), Path::new("/nope/out")).is_empty());
    }
}
