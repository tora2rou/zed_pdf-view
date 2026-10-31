//! Shows Typst errors and warnings in the editor (underlines, hovers and Zed's diagnostics views),
//! the same way a language server's diagnostics are shown.

use std::{
    borrow::Cow,
    collections::{BTreeMap, HashSet},
    path::PathBuf,
};

use gpui::{App, Entity};
use language::DiagnosticSourceKind;
use lsp::{DiagnosticSeverity, LanguageServerId};
use project::{Project, lsp_store::DocumentDiagnosticsUpdate};
use typst_build::{Diagnostic, Level};

/// The id Typst diagnostics are stored under. Zed groups diagnostics by language server; real
/// servers are numbered upwards from 0, so this value stays clear of them.
const TYPST_SERVER_ID: LanguageServerId = LanguageServerId(usize::MAX / 2);

/// The diagnostics of one document published to a project. Publishing replaces the previous set,
/// including clearing files that no longer have problems.
pub(crate) struct EditorDiagnostics {
    project: Entity<Project>,
    published: HashSet<PathBuf>,
}

impl EditorDiagnostics {
    pub(crate) fn new(project: Entity<Project>) -> Self {
        Self {
            project,
            published: HashSet::new(),
        }
    }

    pub(crate) fn publish<'a>(
        &mut self,
        diagnostics: impl IntoIterator<Item = &'a Diagnostic>,
        cx: &mut App,
    ) {
        let mut by_path = to_lsp_diagnostics(diagnostics);
        for stale in self.published.drain() {
            by_path.entry(stale).or_default();
        }
        self.published = by_path
            .iter()
            .filter(|(_, diagnostics)| !diagnostics.is_empty())
            .map(|(path, _)| path.clone())
            .collect();
        let updates: Vec<_> = by_path
            .into_iter()
            .filter_map(|(path, diagnostics)| {
                let uri = lsp::Uri::from_file_path(&path).ok()?;
                Some(DocumentDiagnosticsUpdate {
                    diagnostics: lsp::PublishDiagnosticsParams {
                        uri,
                        diagnostics,
                        version: None,
                    },
                    result_id: None,
                    registration_id: None,
                    server_id: TYPST_SERVER_ID,
                    disk_based_sources: Cow::Borrowed(&[]),
                })
            })
            .collect();
        if updates.is_empty() {
            return;
        }
        let lsp_store = self.project.read(cx).lsp_store();
        lsp_store.update(cx, |lsp_store, cx| {
            // Each update replaces all earlier Typst diagnostics of its file.
            if let Err(error) = lsp_store.merge_lsp_diagnostics(
                DiagnosticSourceKind::Other,
                updates,
                |_, _, _| false,
                cx,
            ) {
                log::warn!("failed to show Typst diagnostics: {error:#}");
            }
        });
    }
}

/// Groups the diagnostics that have a location by file, as LSP diagnostics. LSP positions count
/// columns in UTF-16 units, like Typst's ranges.
fn to_lsp_diagnostics<'a>(
    diagnostics: impl IntoIterator<Item = &'a Diagnostic>,
) -> BTreeMap<PathBuf, Vec<lsp::Diagnostic>> {
    let mut by_path: BTreeMap<PathBuf, Vec<_>> = BTreeMap::new();
    for diagnostic in diagnostics {
        let (Some(path), Some(range)) = (&diagnostic.path, diagnostic.range) else {
            continue;
        };
        let position =
            |(line, column): (usize, usize)| lsp::Position::new(line as u32, column as u32);
        let mut message = diagnostic.message.clone();
        for hint in &diagnostic.hints {
            message.push_str("\nhint: ");
            message.push_str(hint);
        }
        by_path
            .entry(path.clone())
            .or_default()
            .push(lsp::Diagnostic {
                range: lsp::Range::new(position(range.start), position(range.end)),
                severity: Some(match diagnostic.severity {
                    Level::Error => DiagnosticSeverity::ERROR,
                    Level::Warning => DiagnosticSeverity::WARNING,
                }),
                source: Some("typst".to_string()),
                message: message.into(),
                ..lsp::Diagnostic::default()
            });
    }
    by_path
}

#[cfg(test)]
mod tests {
    use super::*;
    use typst_build::Utf16Range;

    fn located(path: &str, start: (usize, usize), end: (usize, usize)) -> Diagnostic {
        Diagnostic {
            path: Some(PathBuf::from(path)),
            range: Some(Utf16Range { start, end }),
            hints: vec!["try this".to_string()],
            ..Diagnostic::error("broken".to_string())
        }
    }

    #[test]
    fn diagnostics_are_grouped_by_file_and_keep_their_hints() {
        let diagnostics = [
            located("/a/main.typ", (3, 1), (3, 8)),
            Diagnostic::error("no location".to_string()),
            located("/a/lib.typ", (0, 0), (0, 2)),
        ];
        let by_path = to_lsp_diagnostics(&diagnostics);
        assert_eq!(
            by_path.len(),
            2,
            "diagnostics without a location are skipped"
        );
        let main = &by_path[&PathBuf::from("/a/main.typ")][0];
        assert_eq!(main.range.start, lsp::Position::new(3, 1));
        assert_eq!(main.range.end, lsp::Position::new(3, 8));
        assert_eq!(main.severity, Some(DiagnosticSeverity::ERROR));
        assert_eq!(main.message.to_string(), "broken\nhint: try this");
    }
}
