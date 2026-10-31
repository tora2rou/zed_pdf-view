//! Debug mode: a dock panel listing the errors and warnings of the Typst document being edited.
//! The panel follows the active editor while it is shown, and lets go of the document (and its
//! compiler) when it is hidden.

use std::path::PathBuf;

use gpui::{
    Action, App, AsyncWindowContext, Context, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement, IntoElement, ParentElement, Pixels, Render, StatefulInteractiveElement,
    Styled, Subscription, Task, WeakEntity, Window, actions, div, px,
};
use typst_build::{Diagnostic, Level, SourcePosition};
use ui::prelude::*;
use workspace::{
    Workspace,
    dock::{DockPosition, Panel, PanelEvent},
};

use crate::{
    active_typst_file,
    document::{TypstDocument, TypstDocumentEvent},
    go_to_source,
};

actions!(
    typst,
    [
        /// Shows or hides the Typst diagnostics panel (debug mode).
        ToggleDiagnosticsPanel,
    ]
);

pub(crate) fn register(workspace: &mut Workspace) {
    workspace.register_action(|workspace, _: &ToggleDiagnosticsPanel, window, cx| {
        workspace.toggle_panel_focus::<TypstDiagnosticsPanel>(window, cx);
    });
}

pub struct TypstDiagnosticsPanel {
    workspace: WeakEntity<Workspace>,
    focus_handle: FocusHandle,
    position: DockPosition,
    active: bool,
    /// The document being shown. Only held while the panel is visible.
    pub(crate) document: Option<Entity<TypstDocument>>,
    document_subscription: Option<Subscription>,
    _workspace_subscription: Subscription,
    jump_task: Task<()>,
}

impl TypstDiagnosticsPanel {
    pub fn load(
        workspace: WeakEntity<Workspace>,
        cx: AsyncWindowContext,
    ) -> Task<anyhow::Result<Entity<Self>>> {
        cx.spawn(async move |cx| {
            workspace.update_in(cx, |workspace, window, cx| {
                let workspace_handle = cx.entity();
                cx.new(|cx| Self::new(&workspace_handle, workspace, window, cx))
            })
        })
    }

    pub(crate) fn new(
        workspace_entity: &Entity<Workspace>,
        workspace: &Workspace,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let workspace_subscription = cx.subscribe_in(
            workspace_entity,
            window,
            |this, _, event: &workspace::Event, _, cx| {
                if let workspace::Event::ActiveItemChanged = event {
                    this.follow_active_editor(cx);
                }
            },
        );
        Self {
            workspace: workspace.weak_handle(),
            focus_handle: cx.focus_handle(),
            position: DockPosition::Right,
            active: false,
            document: None,
            document_subscription: None,
            _workspace_subscription: workspace_subscription,
            jump_task: Task::ready(()),
        }
    }

    /// Shows the document of the Typst file in the active editor. Other kinds of items (such as the
    /// preview itself) keep the current document.
    fn follow_active_editor(&mut self, cx: &mut Context<Self>) {
        if !self.active {
            return;
        }
        let Some(workspace) = self.workspace.upgrade() else {
            return;
        };
        let (path, project) = {
            let workspace = workspace.read(cx);
            let Some(path) = active_typst_file(workspace, cx) else {
                return;
            };
            (path, workspace.project().clone())
        };
        let document = TypstDocument::for_file(&path, &project, cx);
        if self.document.as_ref() != Some(&document) {
            self.set_document(Some(document), cx);
        }
    }

    fn set_document(&mut self, document: Option<Entity<TypstDocument>>, cx: &mut Context<Self>) {
        self.document_subscription = document
            .as_ref()
            .map(|document| cx.subscribe(document, |_, _, _: &TypstDocumentEvent, cx| cx.notify()));
        if let Some(document) = &document {
            document.update(cx, |document, cx| document.ensure_compiled(cx));
        }
        self.document = document;
        cx.notify();
    }

    pub(crate) fn jump_to(
        &mut self,
        path: PathBuf,
        line: usize,
        column: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let workspace = self.workspace.clone();
        let position = SourcePosition { path, line, column };
        self.jump_task = cx.spawn_in(window, async move |_, cx| {
            go_to_source(workspace, position, None, cx).await;
        });
    }

    fn render_header(&self, cx: &App) -> impl IntoElement {
        let (title, summary, color) = match &self.document {
            None => (
                "Typst Diagnostics".to_string(),
                "Open a .typ file to see its errors and warnings.".to_string(),
                Color::Muted,
            ),
            Some(document) => {
                let document = document.read(cx);
                let title = document
                    .main()
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let (summary, color) = summary(
                    document.errors().len(),
                    document.warnings().len(),
                    document.is_compiling() || !document.has_compiled(),
                );
                (title, summary, color)
            }
        };
        v_flex()
            .px_2()
            .py_1()
            .gap_0p5()
            .border_b_1()
            .border_color(cx.theme().colors().border)
            .child(Label::new(title).single_line())
            .child(Label::new(summary).size(LabelSize::Small).color(color))
    }

    fn render_row(
        &self,
        index: usize,
        diagnostic: &Diagnostic,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let (icon, color) = match diagnostic.severity {
            Level::Error => (IconName::XCircle, Color::Error),
            Level::Warning => (IconName::Warning, Color::Warning),
        };
        let location = match (&diagnostic.file, diagnostic.line, diagnostic.column) {
            (Some(file), Some(line), Some(column)) => Some(format!("{file}:{line}:{column}")),
            (Some(file), _, _) => Some(file.clone()),
            _ => None,
        };
        let target = match (&diagnostic.path, diagnostic.line, diagnostic.column) {
            (Some(path), Some(line), Some(column)) => Some((path.clone(), line, column)),
            _ => None,
        };
        h_flex()
            .id(("typst-diagnostic", index))
            .w_full()
            .items_start()
            .gap_1p5()
            .px_2()
            .py_1()
            .when(target.is_some(), |row| {
                row.cursor_pointer()
                    .hover(|style| style.bg(cx.theme().colors().element_hover))
            })
            .when_some(target, |row, (path, line, column)| {
                row.on_click(cx.listener(move |this, _, window, cx| {
                    this.jump_to(path.clone(), line, column, window, cx);
                }))
            })
            .child(Icon::new(icon).size(IconSize::Small).color(color))
            .child(
                v_flex()
                    .min_w_0()
                    .flex_1()
                    .child(Label::new(diagnostic.message.clone()))
                    .when_some(location, |column, location| {
                        column.child(
                            Label::new(location)
                                .size(LabelSize::Small)
                                .color(Color::Muted),
                        )
                    })
                    .children(diagnostic.hints.iter().map(|hint| {
                        Label::new(format!("hint: {hint}"))
                            .size(LabelSize::Small)
                            .color(Color::Muted)
                    })),
            )
    }
}

/// The one-line summary under the panel title, and its color.
fn summary(errors: usize, warnings: usize, compiling: bool) -> (String, Color) {
    if compiling && errors == 0 && warnings == 0 {
        return ("Compiling…".to_string(), Color::Muted);
    }
    let plural = |count: usize, noun: &str| match count {
        1 => format!("1 {noun}"),
        count => format!("{count} {noun}s"),
    };
    match (errors, warnings) {
        (0, 0) => ("No problems".to_string(), Color::Success),
        (0, warnings) => (plural(warnings, "warning"), Color::Warning),
        (errors, 0) => (plural(errors, "error"), Color::Error),
        (errors, warnings) => (
            format!(
                "{}, {}",
                plural(errors, "error"),
                plural(warnings, "warning")
            ),
            Color::Error,
        ),
    }
}

impl Render for TypstDiagnosticsPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let diagnostics: Vec<Diagnostic> = self
            .document
            .as_ref()
            .map(|document| {
                let document = document.read(cx);
                document
                    .errors()
                    .iter()
                    .chain(document.warnings())
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        let rows: Vec<_> = diagnostics
            .iter()
            .enumerate()
            .map(|(index, diagnostic)| self.render_row(index, diagnostic, cx))
            .collect();
        v_flex()
            .key_context("TypstDiagnosticsPanel")
            .track_focus(&self.focus_handle)
            .size_full()
            .bg(cx.theme().colors().panel_background)
            .child(self.render_header(cx))
            .child(
                div()
                    .id("typst-diagnostics")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(v_flex().children(rows)),
            )
    }
}

impl Focusable for TypstDiagnosticsPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<PanelEvent> for TypstDiagnosticsPanel {}

impl Panel for TypstDiagnosticsPanel {
    fn persistent_name() -> &'static str {
        "TypstDiagnosticsPanel"
    }

    fn panel_key() -> &'static str {
        "TypstDiagnosticsPanel"
    }

    fn position(&self, _window: &Window, _cx: &App) -> DockPosition {
        self.position
    }

    fn position_is_valid(&self, _position: DockPosition) -> bool {
        true
    }

    fn set_position(
        &mut self,
        position: DockPosition,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.position = position;
        cx.notify();
    }

    fn default_size(&self, _window: &Window, _cx: &App) -> Pixels {
        px(320.)
    }

    fn icon(&self, _window: &Window, _cx: &App) -> Option<IconName> {
        Some(IconName::Warning)
    }

    fn icon_tooltip(&self, _window: &Window, _cx: &App) -> Option<&'static str> {
        Some("Typst Diagnostics")
    }

    fn toggle_action(&self) -> Box<dyn Action> {
        Box::new(ToggleDiagnosticsPanel)
    }

    fn icon_label(&self, _window: &Window, cx: &App) -> Option<String> {
        let document = self.document.as_ref()?.read(cx);
        let count = document.errors().len() + document.warnings().len();
        (count > 0).then(|| count.to_string())
    }

    fn activation_priority(&self) -> u32 {
        // After the built-in panels.
        20
    }

    fn set_active(&mut self, active: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.active = active;
        if active {
            // The workspace is being updated while it shows the panel, so it cannot be read yet.
            cx.defer_in(window, |this, _, cx| this.follow_active_editor(cx));
        } else {
            self.set_document(None, cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_counts_problems() {
        assert_eq!(summary(0, 0, true).0, "Compiling…");
        assert_eq!(summary(0, 0, false).0, "No problems");
        assert_eq!(summary(1, 0, false).0, "1 error");
        assert_eq!(summary(0, 2, false).0, "2 warnings");
        assert_eq!(summary(2, 1, true).0, "2 errors, 1 warning");
    }
}
