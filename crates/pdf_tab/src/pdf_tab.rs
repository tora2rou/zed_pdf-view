//! Opens `.pdf` files in Zed's workspace as tabs, shown by Chihayaburu's `PdfView`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Context as _;
use file_icons::FileIcons;
use gpui::AppContext as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, Context, Entity, EventEmitter, FocusHandle, Focusable, IntoElement,
    ParentElement, Render, SharedString, Styled, Task, Window, div,
};
use pdf_core::PdfDocument;
use pdf_viewer::PdfView;
use project::{Project, ProjectEntryId, ProjectPath};
use settings::Settings as _;
use ui::{Icon, Label, LabelCommon};
use util::paths::PathExt;
use workspace::{
    ItemSettings, Pane,
    item::{Item, ItemBufferKind, ProjectItem, TabContentParams},
};

const PDF_EXTENSION: &str = "pdf";

pub fn init(cx: &mut App) {
    workspace::register_project_item::<PdfTab>(cx);
}

fn is_pdf(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case(PDF_EXTENSION))
}

/// A PDF file in the project. The document is loaded once and shared by every tab showing it.
pub struct PdfItem {
    project_path: ProjectPath,
    abs_path: PathBuf,
    entry_id: Option<ProjectEntryId>,
    document: Arc<PdfDocument>,
}

impl project::ProjectItem for PdfItem {
    fn try_open(
        project: &Entity<Project>,
        path: &ProjectPath,
        cx: &mut App,
    ) -> Option<Task<anyhow::Result<Entity<Self>>>> {
        // For single-file worktrees the relative path is empty, so check the absolute path.
        let abs_path = project.read(cx).absolute_path(path, cx)?;
        if !is_pdf(&abs_path) {
            return None;
        }
        let project_path = path.clone();
        let entry_id = project
            .read(cx)
            .entry_for_path(&project_path, cx)
            .map(|entry| entry.id);
        Some(cx.spawn(async move |cx| {
            let document = cx
                .background_spawn({
                    let abs_path = abs_path.clone();
                    async move { PdfDocument::open(&abs_path) }
                })
                .await
                .with_context(|| format!("opening PDF {}", abs_path.display()))?;
            Ok(cx.new(|_| PdfItem {
                project_path,
                abs_path,
                entry_id,
                document: Arc::new(document),
            }))
        }))
    }

    fn entry_id(&self, _: &App) -> Option<ProjectEntryId> {
        self.entry_id
    }

    fn project_path(&self, _: &App) -> Option<ProjectPath> {
        Some(self.project_path.clone())
    }

    fn is_dirty(&self) -> bool {
        false
    }
}

/// A workspace tab that displays a [`PdfItem`].
pub struct PdfTab {
    item: Entity<PdfItem>,
    viewer: Entity<PdfView>,
}

impl PdfTab {
    fn new(item: Entity<PdfItem>, cx: &mut Context<Self>) -> Self {
        let document = item.read(cx).document.clone();
        let viewer = cx.new(|cx| PdfView::new(document, cx));
        Self { item, viewer }
    }
}

impl EventEmitter<()> for PdfTab {}

impl Focusable for PdfTab {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.viewer.focus_handle(cx)
    }
}

impl Render for PdfTab {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.viewer.clone())
    }
}

impl Item for PdfTab {
    type Event = ();

    fn for_each_project_item(
        &self,
        cx: &App,
        f: &mut dyn FnMut(gpui::EntityId, &dyn project::ProjectItem),
    ) {
        f(self.item.entity_id(), self.item.read(cx))
    }

    fn tab_content_text(&self, _detail: usize, cx: &App) -> SharedString {
        self.item
            .read(cx)
            .abs_path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned().into())
            .unwrap_or_else(|| "PDF".into())
    }

    fn tab_content(&self, params: TabContentParams, _window: &Window, cx: &App) -> AnyElement {
        Label::new(self.tab_content_text(params.detail.unwrap_or_default(), cx))
            .single_line()
            .color(params.text_color())
            .when(params.preview, |this| this.italic())
            .into_any_element()
    }

    fn tab_tooltip_text(&self, cx: &App) -> Option<SharedString> {
        let abs_path = self.item.read(cx).abs_path.compact();
        Some(abs_path.to_string_lossy().into_owned().into())
    }

    fn tab_icon(&self, _window: &Window, cx: &App) -> Option<Icon> {
        let abs_path = &self.item.read(cx).abs_path;
        ItemSettings::get_global(cx)
            .file_icons
            .then(|| FileIcons::get_icon(abs_path, cx))
            .flatten()
            .map(Icon::from_path)
    }

    fn buffer_kind(&self, _: &App) -> ItemBufferKind {
        ItemBufferKind::Singleton
    }
}

impl ProjectItem for PdfTab {
    type Item = PdfItem;

    fn for_project_item(
        _project: Entity<Project>,
        _pane: Option<&Pane>,
        item: Entity<Self::Item>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::new(item, cx)
    }
}

#[cfg(test)]
mod tests {
    use super::is_pdf;
    use std::path::Path;

    #[test]
    fn detects_pdf_extension_case_insensitively() {
        assert!(is_pdf(Path::new("book.pdf")));
        assert!(is_pdf(Path::new("dir/BOOK.PDF")));
        assert!(!is_pdf(Path::new("book.typ")));
        assert!(!is_pdf(Path::new("pdf")));
    }
}
