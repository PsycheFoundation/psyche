{
  inputs,
  system ? null,
  lib ? inputs.nixpkgs.lib,
}:
lib.optionalAttrs (system != null) { inherit system; }
// {
  overlays = [
    inputs.rust-overlay.overlays.default
    inputs.nix-gl-host.overlays.default

    (
      final: prev:
      import ./pkgs.nix {
        pkgs = prev;
        inherit inputs;
      }
    )
  ];

  config = {
    allowUnfree = true;
    cudaSupport = true;
    cudaVersion = "12.4";
  };
}
