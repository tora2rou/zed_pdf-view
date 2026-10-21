{
  description = "Chihayaburu - a fast, simple PDF viewer built with GPUI";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    # rust-toolchain.toml が指定するRust(1.98.1)を使うために必要。nixpkgsのRustは、古いことがある。
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    { self, nixpkgs, rust-overlay }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];
      forAllSystems =
        f:
        nixpkgs.lib.genAttrs systems (
          system:
          f (
            import nixpkgs {
              inherit system;
              overlays = [ rust-overlay.overlays.default ];
            }
          )
        );
      rustToolchainFor = pkgs: pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
    in
    {
      packages = forAllSystems (pkgs: rec {
        chihayaburu = pkgs.callPackage ./nix/package.nix { rustToolchain = rustToolchainFor pkgs; };
        default = chihayaburu;
      });

      apps = forAllSystems (pkgs: rec {
        chihayaburu = {
          type = "app";
          program = "${self.packages.${pkgs.system}.chihayaburu}/bin/chihayaburu";
        };
        default = chihayaburu;
      });

      devShells = forAllSystems (pkgs: {
        default = pkgs.mkShell {
          packages = [
            (rustToolchainFor pkgs)
            pkgs.pkg-config
            pkgs.cmake
          ];
          buildInputs = self.packages.${pkgs.system}.chihayaburu.buildInputs;
          # `cargo run` で、Wayland・Vulkanなど実行時に読み込まれるライブラリが見つかるように
          LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath self.packages.${pkgs.system}.chihayaburu.buildInputs;
        };
      });
    };
}
