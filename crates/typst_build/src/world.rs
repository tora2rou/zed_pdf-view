//! コンパイルに必要な環境(ファイル・フォント・パッケージ・日付)を与える`World`。

use std::{
    path::{Path, PathBuf},
    sync::OnceLock,
};

use typst::{
    Library, LibraryExt as _, World,
    diag::FileResult,
    foundations::{Bytes, Datetime, Duration},
    syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot},
    text::{Font, FontBook},
    utils::LazyHash,
};
use typst_kit::{
    datetime::Time,
    downloader::SystemDownloader,
    files::{FileStore, FsRoot, SystemFiles},
    fonts::{self, FontStore},
    packages::SystemPackages,
};

/// ダウンロードのときに名乗る名前。
const USER_AGENT: &str = concat!("chihayaburu/", env!("CARGO_PKG_VERSION"));

/// 標準ライブラリ。作るのが重いので、一度だけ作る。
fn library() -> &'static LazyHash<Library> {
    static LIBRARY: OnceLock<LazyHash<Library>> = OnceLock::new();
    LIBRARY.get_or_init(|| LazyHash::new(Library::default()))
}

/// フォント。システムのフォントを調べるのは遅いので、一度だけ調べる。
fn font_store() -> &'static FontStore {
    static FONTS: OnceLock<FontStore> = OnceLock::new();
    FONTS.get_or_init(|| {
        let mut store = FontStore::new();
        // 同梱のフォントを先に。システムのフォントに同名があっても、同梱のものが優先される。
        store.extend(fonts::embedded());
        store.extend(fonts::system());
        store
    })
}

pub struct FileWorld {
    root: PathBuf,
    main: FileId,
    files: FileStore<SystemFiles>,
    time: Time,
}

impl FileWorld {
    /// `main`をメインのファイルとする環境。プロジェクトのルートは`main`のあるフォルダ。
    pub fn new(main: &Path) -> Result<Self, String> {
        let main = main
            .canonicalize()
            .map_err(|e| format!("ファイルを開けません: {e}"))?;
        let root = main
            .parent()
            .ok_or("ファイルの置かれたフォルダが分かりません")?
            .to_path_buf();
        let vpath = VirtualPath::virtualize(&root, &main)
            .map_err(|e| format!("パスを解釈できません: {e:?}"))?;
        let packages = SystemPackages::new(SystemDownloader::new(USER_AGENT));
        Ok(Self {
            main: RootedPath::new(VirtualRoot::Project, vpath).intern(),
            files: FileStore::new(SystemFiles::new(FsRoot::new(root.clone()), packages)),
            root,
            time: Time::system(),
        })
    }

    /// コンパイル中に読み込んだ、プロジェクト内のファイル(読めなかったものも含む)。メインのファイルは
    /// 必ず入る。パッケージのファイルは含まない。
    pub fn dependencies(&mut self) -> Vec<PathBuf> {
        let main = self.main;
        let (loader, ids) = self.files.dependencies();
        let mut paths: Vec<PathBuf> = ids
            .chain([main])
            .filter(|id| matches!(id.root(), VirtualRoot::Project))
            .filter_map(|id| loader.resolve(id).ok())
            .collect();
        paths.sort();
        paths.dedup();
        paths
    }

    /// 診断の表示用のファイル名。プロジェクト内のファイルはルートからの相対パス。
    pub fn display_name(&self, id: FileId) -> String {
        match self.files.loader().resolve(id) {
            Ok(path) => path
                .strip_prefix(&self.root)
                .unwrap_or(&path)
                .display()
                .to_string(),
            Err(_) => format!("{:?}", id.vpath()),
        }
    }
}

impl World for FileWorld {
    fn library(&self) -> &LazyHash<Library> {
        library()
    }

    fn book(&self) -> &LazyHash<FontBook> {
        font_store().book()
    }

    fn main(&self) -> FileId {
        self.main
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        self.files.source(id)
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        self.files.file(id)
    }

    fn font(&self, index: usize) -> Option<Font> {
        font_store().font(index)
    }

    fn today(&self, offset: Option<Duration>) -> Option<Datetime> {
        self.time.today(offset)
    }
}
