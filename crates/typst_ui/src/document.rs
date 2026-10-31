//! A Typst document shared by everything that shows it (previews and the diagnostics panel), so a
//! file is compiled once no matter how many views observe it.

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{Arc, Weak},
    time::Duration,
};

use gpui::{
    App, AppContext as _, Context, Entity, EntityId, EventEmitter, Global, Subscription, Task,
    WeakEntity,
};
use language::{Buffer, BufferEvent};
use parking_lot::Mutex;
use pdf_core::PdfDocument;
use project::{Project, buffer_store::BufferStoreEvent};
use typst_build::{Diagnostic, Session, SourcePosition};

use crate::{buffer_abs_path, normalize};

/// How long the text must stay unchanged before it is compiled.
pub(crate) const DEBOUNCE: Duration = Duration::from_millis(300);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypstDocumentEvent {
    /// A compilation started or finished, so the errors, warnings or PDF may have changed.
    Updated,
}

/// Held by a view that shows the PDF. While any request is alive, compilations also produce a PDF;
/// otherwise only errors and warnings are checked, which is faster and uses less memory.
pub struct PdfRequest(#[allow(dead_code)] Arc<()>);

/// Live documents, by project and resolved main path.
#[derive(Default)]
struct Registry(HashMap<(EntityId, PathBuf), WeakEntity<TypstDocument>>);

impl Global for Registry {}

pub struct TypstDocument {
    main: PathBuf,
    project: Entity<Project>,
    session: Option<Arc<Mutex<Session>>>,
    /// Files read by the last compilation (resolved paths). Edits to these trigger a recompile.
    dependencies: HashSet<PathBuf>,
    errors: Vec<Diagnostic>,
    warnings: Vec<Diagnostic>,
    /// The output of the last successful compilation that produced a PDF.
    pdf: Option<Arc<PdfDocument>>,
    /// Whether `pdf` reflects the latest successful compilation.
    pdf_current: bool,
    pdf_requests: Vec<Weak<()>>,
    has_compiled: bool,
    compiling: bool,
    /// An edit or request arrived while compiling, so compile again once the current run finishes.
    recompile: bool,
    buffer_subscriptions: HashMap<EntityId, Subscription>,
    _buffer_store_subscription: Subscription,
    debounce: Task<()>,
    compile_task: Task<()>,
}

impl EventEmitter<TypstDocumentEvent> for TypstDocument {}

impl TypstDocument {
    /// The document whose main file is `main`, created if nothing has it open yet.
    pub fn for_main(main: &Path, project: &Entity<Project>, cx: &mut App) -> Entity<Self> {
        let key = (project.entity_id(), normalize(main));
        if let Some(document) = cx
            .default_global::<Registry>()
            .0
            .get(&key)
            .and_then(WeakEntity::upgrade)
        {
            return document;
        }
        let document = cx.new(|cx| Self::new(main.to_path_buf(), project.clone(), cx));
        let registry = cx.default_global::<Registry>();
        registry
            .0
            .retain(|_, document| document.upgrade().is_some());
        registry.0.insert(key, document.downgrade());
        document
    }

    /// The document `path` belongs to: its own if it is open as a main file, else an open document
    /// that reads it (such as one importing it), else a new document with `path` as the main file.
    pub fn for_file(path: &Path, project: &Entity<Project>, cx: &mut App) -> Entity<Self> {
        let path = normalize(path);
        let project_id = project.entity_id();
        let open: Vec<(PathBuf, Entity<Self>)> = cx
            .default_global::<Registry>()
            .0
            .iter()
            .filter(|((project, _), _)| *project == project_id)
            .filter_map(|((_, main), document)| Some((main.clone(), document.upgrade()?)))
            .collect();
        let own = open.iter().find(|(main, _)| *main == path);
        let including = open
            .iter()
            .find(|(_, document)| document.read(cx).dependencies.contains(&path));
        match own.or(including) {
            Some((_, document)) => document.clone(),
            None => Self::for_main(&path, project, cx),
        }
    }

    fn new(main: PathBuf, project: Entity<Project>, cx: &mut Context<Self>) -> Self {
        let (session, errors) = match Session::new(&main) {
            Ok(session) => (Some(Arc::new(Mutex::new(session))), Vec::new()),
            Err(failure) => (None, failure.errors),
        };
        let buffer_store = project.read(cx).buffer_store().clone();
        let buffer_store_subscription =
            cx.subscribe(&buffer_store, |this, _, event: &BufferStoreEvent, cx| {
                if let BufferStoreEvent::BufferAdded(_) = event {
                    this.watch_buffers(cx);
                }
            });
        let mut this = Self {
            dependencies: HashSet::from([normalize(&main)]),
            main,
            project,
            session,
            errors,
            warnings: Vec::new(),
            pdf: None,
            pdf_current: false,
            pdf_requests: Vec::new(),
            has_compiled: false,
            compiling: false,
            recompile: false,
            buffer_subscriptions: HashMap::new(),
            _buffer_store_subscription: buffer_store_subscription,
            debounce: Task::ready(()),
            compile_task: Task::ready(()),
        };
        this.watch_buffers(cx);
        this
    }

    pub fn main(&self) -> &Path {
        &self.main
    }

    pub fn errors(&self) -> &[Diagnostic] {
        &self.errors
    }

    pub fn warnings(&self) -> &[Diagnostic] {
        &self.warnings
    }

    pub fn is_compiling(&self) -> bool {
        self.compiling
    }

    pub fn has_compiled(&self) -> bool {
        self.has_compiled
    }

    /// The latest PDF. While the document has errors, this is the last one that compiled.
    pub fn pdf(&self) -> Option<&Arc<PdfDocument>> {
        self.pdf.as_ref()
    }

    /// Asks for the PDF to be produced and kept up to date while the returned request is alive.
    pub fn request_pdf(&mut self, cx: &mut Context<Self>) -> PdfRequest {
        let request = Arc::new(());
        self.pdf_requests.push(Arc::downgrade(&request));
        if !self.pdf_current {
            self.compile(cx);
        }
        PdfRequest(request)
    }

    /// Compiles once if nothing has been compiled yet.
    pub fn ensure_compiled(&mut self, cx: &mut Context<Self>) {
        if !self.has_compiled {
            self.compile(cx);
        }
    }

    /// The source that produced the text at (`x`, `y`) points on `page` of the shown PDF.
    pub fn source_at(&self, page: usize, x: f32, y: f32, cx: &App) -> Task<Option<SourcePosition>> {
        let Some(session) = self.session.clone() else {
            return Task::ready(None);
        };
        // The session is locked while compiling, so look the position up off the UI thread.
        cx.background_spawn(async move { session.lock().source_at(page, x, y) })
    }

    fn wants_pdf(&mut self) -> bool {
        self.pdf_requests
            .retain(|request| request.strong_count() > 0);
        !self.pdf_requests.is_empty()
    }

    /// Open buffers of the files this document depends on, with their resolved paths.
    fn dependency_buffers(&self, cx: &App) -> Vec<(Entity<Buffer>, PathBuf)> {
        self.project
            .read(cx)
            .buffer_store()
            .read(cx)
            .buffers()
            .filter_map(|buffer| {
                let path = normalize(&buffer_abs_path(&buffer, cx)?);
                self.dependencies.contains(&path).then_some((buffer, path))
            })
            .collect()
    }

    /// Subscribes to edits of the open buffers this document depends on.
    fn watch_buffers(&mut self, cx: &mut Context<Self>) {
        let buffers = self.dependency_buffers(cx);
        let watched: HashSet<EntityId> = buffers
            .iter()
            .map(|(buffer, _)| buffer.entity_id())
            .collect();
        self.buffer_subscriptions
            .retain(|id, _| watched.contains(id));
        for (buffer, _) in buffers {
            self.buffer_subscriptions
                .entry(buffer.entity_id())
                .or_insert_with(|| {
                    cx.subscribe(&buffer, |this, _, event: &BufferEvent, cx| {
                        if matches!(event, BufferEvent::Edited { .. } | BufferEvent::Reloaded) {
                            this.schedule_compile(cx);
                        }
                    })
                });
        }
    }

    fn schedule_compile(&mut self, cx: &mut Context<Self>) {
        self.debounce = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(DEBOUNCE).await;
            this.update(cx, |this, cx| this.compile(cx)).ok();
        });
    }

    fn compile(&mut self, cx: &mut Context<Self>) {
        let Some(session) = self.session.clone() else {
            return;
        };
        if self.compiling {
            self.recompile = true;
            return;
        }
        // Only unsaved text needs to be passed; everything else is read from disk.
        let overlays: HashMap<PathBuf, String> = self
            .dependency_buffers(cx)
            .into_iter()
            .filter(|(buffer, _)| buffer.read(cx).is_dirty())
            .map(|(buffer, path)| (path, buffer.read(cx).text()))
            .collect();
        let export_pdf = self.wants_pdf();
        self.compiling = true;
        cx.emit(TypstDocumentEvent::Updated);
        cx.notify();

        let compile = cx.background_spawn(async move {
            let result = session.lock().compile(overlays, export_pdf);
            result.map(|compiled| {
                let pdf = export_pdf.then(|| PdfDocument::from_bytes(compiled.pdf).map(Arc::new));
                (pdf, compiled.warnings, compiled.dependencies)
            })
        });
        self.compile_task = cx.spawn(async move |this, cx| {
            let result = compile.await;
            this.update(cx, |this, cx| {
                this.compiling = false;
                this.has_compiled = true;
                let dependencies = match result {
                    Ok((pdf, warnings, dependencies)) => {
                        this.errors.clear();
                        this.warnings = warnings;
                        match pdf {
                            Some(Ok(pdf)) => this.pdf = Some(pdf),
                            Some(Err(error)) => this.errors.push(Diagnostic::error(format!(
                                "The compiled PDF could not be opened: {error}"
                            ))),
                            // Nobody shows the PDF, so free it instead of keeping a stale copy.
                            None => this.pdf = None,
                        }
                        this.pdf_current = export_pdf;
                        dependencies
                    }
                    Err(failure) => {
                        this.errors = failure.errors;
                        this.warnings = failure.warnings;
                        failure.dependencies
                    }
                };
                this.dependencies = dependencies.iter().map(|path| normalize(path)).collect();
                this.dependencies.insert(normalize(&this.main));
                this.watch_buffers(cx);
                if std::mem::take(&mut this.recompile) {
                    this.compile(cx);
                }
                cx.emit(TypstDocumentEvent::Updated);
                cx.notify();
            })
            .ok();
        });
    }
}
