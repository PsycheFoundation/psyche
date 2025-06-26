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
      python312Packages = prev.python312Packages.override {
        overrides = pyfinal: pyprev: rec {
          torch = pyprev.torch-bin;
        };
      };
    })
  ];

  config = {
    allowUnfree = true;
    cudaSupport = true;
    cudaVersion = "12.8";
  };
}
