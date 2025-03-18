{ gitcommit, inputs }:
{
  overlays = [
    (import inputs.rust-overlay)
    inputs.nix-gl-host.overlays.default
    (final: prev: {
      psycheLib = import ./common.nix {
        inherit inputs gitcommit;
        system = final.system;
        pkgs = final;
      };
    })
  ];

  config = {
    allowUnfree = true;
    cudaSupport = true;
    cudaVersion = "12.4";
  };
}
