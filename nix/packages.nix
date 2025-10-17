{ lib, inputs, ... }:
{
  perSystem =
    {
      system,
      pkgs,
      inputs',
      pythonSet,
      ...
    }:
    let
      inherit (pkgs.psycheLib) buildRustPackageWithPsychePythonEnvironment useHostGpuDrivers;

    in
    {
      _module.args.pkgs = import inputs.nixpkgs (
        import ./nixpkgs.nix {
          inherit inputs system pythonSet;
        }
      );

      packages = {
        flattenReferencesGraph = pkgs.flattenReferencesGraph;
      }
      // lib.mapAttrs (_: lib.id) pkgs.psychePackages;
    };
}
