{
  lib,
  makeRustPlatform,
  rustToolchain,
  makeWrapper,
  pkg-config,
  cmake,
  fontconfig,
  freetype,
  libxcb,
  libxkbcommon,
  libGL,
  vulkan-loader,
  wayland,
  xdg-utils,
}:

let
  rustPlatform = makeRustPlatform {
    cargo = rustToolchain;
    rustc = rustToolchain;
  };

  # GPUIは、Wayland・Vulkan・OpenGL・フォント関係のライブラリを、実行時に読み込む(dlopen)ものがある。
  # ビルドに必要というより、実行時に見つかる必要があるので、起動スクリプトの LD_LIBRARY_PATH に足す。
  runtimeLibs = [
    fontconfig
    freetype
    libGL
    libxcb
    libxkbcommon
    vulkan-loader
    wayland
  ];
in
rustPlatform.buildRustPackage {
  pname = "chihayaburu";
  version = (builtins.fromTOML (builtins.readFile ../Cargo.toml)).workspace.package.version;

  # ビルドに要るものだけを取り込む(target/ やメモなどでキャッシュが無効にならないように)
  src = lib.fileset.toSource {
    root = ../.;
    fileset = lib.fileset.unions [
      ../Cargo.toml
      ../Cargo.lock
      ../rust-toolchain.toml
      ../crates
      ../packaging/linux
    ];
  };

  cargoLock = {
    lockFile = ../Cargo.lock;
    # GPUI などはgitの依存(zed-industries/zed の固定されたrev)。Cargo.lock のrevを使って取得する。
    allowBuiltinFetchGit = true;
  };

  cargoBuildFlags = [
    "-p"
    "pdf_view_app"
  ];

  nativeBuildInputs = [
    pkg-config
    cmake
    makeWrapper
  ];
  buildInputs = runtimeLibs;

  # テストはGPUIの描画を試すので、サンドボックスでは動かさない。
  doCheck = false;

  postInstall = ''
    # 実行ファイルの名前を chihayaburu にして、デスクトップファイル・アイコンなどを入れる
    mv $out/bin/pdf_view_app $out/bin/chihayaburu
    make -C packaging/linux install-data PREFIX=$out
    substituteInPlace $out/share/applications/io.github.tora2rou.Chihayaburu.desktop \
      --replace-fail "Exec=chihayaburu" "Exec=$out/bin/chihayaburu"

    wrapProgram $out/bin/chihayaburu \
      --prefix LD_LIBRARY_PATH : ${lib.makeLibraryPath runtimeLibs} \
      --prefix PATH : ${lib.makeBinPath [ xdg-utils ]}
  '';

  meta = {
    description = "A fast, simple PDF viewer built with GPUI";
    homepage = "https://github.com/tora2rou/zed_pdf-view";
    license = lib.licenses.asl20;
    mainProgram = "chihayaburu";
    platforms = [
      "x86_64-linux"
      "aarch64-linux"
    ];
  };
}
