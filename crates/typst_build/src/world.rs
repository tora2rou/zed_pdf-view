//! コンパイルに必要な環境(ファイル・フォント・パッケージ・日付)を与える`World`。

use std::{
    collections::HashMap,
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
    files::{FileLoader, FileStore, FsRoot, SystemFiles},
    fonts::{self, FontPath, FontSource, FontStore},
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
        store.extend(fonts::system().map(|(path, info)| (MappedFont(path), info)));
        store
    })
}

/// システムのフォント。`typst-kit`はファイル全体をメモリに読み込むが、日本語のフォントは1つで
/// 数十MBあるので、メモリマップで読む。使ったところだけがメモリに載り、足りなくなればOSが捨てて
/// 読み直せる (PDFの読み込みと同じ方針)。
struct MappedFont(FontPath);

impl FontSource for MappedFont {
    fn load(&self) -> Option<Font> {
        let file = std::fs::File::open(&self.0.path).ok()?;
        // SAFETY: 開いている間に他のプログラムがフォントのファイルを短くすると、不正なアクセスに
        // なりうる。システムのフォントは普通は書き換えられないので、許容する。
        let data = match unsafe { memmap2::Mmap::map(&file) } {
            Ok(map) => Bytes::new(map),
            Err(_) => Bytes::new(std::fs::read(&self.0.path).ok()?),
        };
        Font::new(data, self.0.index)
    }
}

/// プロジェクトのファイルを読む。エディタで編集中(未保存)のファイルは、ディスクの代わりにその内容を返す。
struct OverlayFiles {
    system: SystemFiles,
    /// ファイルの場所(正規化したもの) → 編集中の内容。
    overlays: HashMap<PathBuf, Bytes>,
}

impl OverlayFiles {
    fn resolve(&self, id: FileId) -> FileResult<PathBuf> {
        self.system.resolve(id)
    }
}

impl FileLoader for OverlayFiles {
    fn load(&self, id: FileId) -> FileResult<Bytes> {
        // パッケージのファイルは編集されない。場所を調べるとダウンロードが始まることもあるので、見ない。
        if matches!(id.root(), VirtualRoot::Project)
            && let Ok(path) = self.system.resolve(id)
            && let Some(text) = self.overlays.get(&path)
        {
            return Ok(text.clone());
        }
        self.system.load(id)
    }
}

/// 比べられるように、ファイルの場所を正規化する。まだディスクに無いファイルは、そのまま。
fn normalize(path: PathBuf) -> PathBuf {
    path.canonicalize().unwrap_or(path)
}

pub struct FileWorld {
    root: PathBuf,
    main: FileId,
    files: FileStore<OverlayFiles>,
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
            files: FileStore::new(OverlayFiles {
                system: SystemFiles::new(FsRoot::new(root.clone()), packages),
                overlays: HashMap::new(),
            }),
            root,
            time: Time::system(),
        })
    }

    /// 次のコンパイルの準備をする。`overlays`(ファイルの場所 → 編集中の内容)はディスクより優先する。
    /// 読み込んだファイルは捨てずに「古い」印を付けるので、変わっていない部分は前回の結果を使い回せる。
    pub fn prepare(&mut self, overlays: HashMap<PathBuf, String>) {
        self.files.loader_mut().overlays = overlays
            .into_iter()
            .map(|(path, text)| (normalize(path), Bytes::from_string(text)))
            .collect();
        self.files.reset();
        self.time = Time::system();
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

    /// 直前のコンパイルで読み込んだソース(パッケージのものも含む)。
    pub fn sources_used(&mut self) -> HashMap<FileId, Source> {
        let ids: Vec<FileId> = self.files.dependencies().1.collect();
        ids.into_iter()
            .filter_map(|id| Some((id, self.files.source(id).ok()?)))
            .collect()
    }

    /// `id`のファイルの、ファイルシステム上の場所。
    pub fn real_path(&self, id: FileId) -> Option<PathBuf> {
        self.files.loader().resolve(id).ok()
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

impl typst_ide::IdeWorld for FileWorld {
    fn upcast(&self) -> &dyn World {
        self
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
