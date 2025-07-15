{ lib, inputs, ... }:
{
  perSystem =
    {
      system,
      pkgs,
      inputs',
      ...
    }:
    let
      inherit (pkgs.psycheLib) buildRustPackageWithPythonSidecar useHostGpuDrivers;

    in
    {
      _module.args.pkgs = import inputs.nixpkgs (
        import ./nixpkgs.nix {
          inherit inputs system;
        }
      );

      packages = lib.mapAttrs (_: lib.id) pkgs.psychePackages;
    };
}
