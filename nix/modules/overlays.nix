{ inputs, ... }:
{
  flake.overlays.default =
    final: _:
    let
      mkTuzuri = import ../toolchain.nix { inherit inputs; };
    in
    {
      tuzuri = mkTuzuri final;
    };
}
