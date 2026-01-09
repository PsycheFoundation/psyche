{
  lib,
  stdenvNoCC,
  mdbook,
  mdbook-mermaid,
  mdbook-linkcheck,
  fetchFromGitHub,

  # custom args
  rustPackages,
}:
let
  # Extract binary package names from rustPackages
  # Strategy: find all packages with -nopython suffix, extract base names
  allPackageNames = builtins.attrNames rustPackages;
  noPythonPackages = builtins.filter (name: lib.hasSuffix "-nopython" name) allPackageNames;
  binaryPackageNames = builtins.map (name: lib.removeSuffix "-nopython" name) noPythonPackages;
in
stdenvNoCC.mkDerivation {
  __structuredAttrs = true;

  name = "psyche-book";
  src = ./.;

  nativeBuildInputs = [
    mdbook
    mdbook-mermaid
    (mdbook-linkcheck.overrideAttrs (
      final: prev: {
        version = "unstable-2025-12-04";
        src = fetchFromGitHub {
          owner = "schilkp";
          repo = "mdbook-linkcheck";
          rev = "ed981be6ded11562e604fff290ae4c08f1c419c5";
          sha256 = "sha256-GTVWc/vkqY9Hml2fmm3iCHOzd/HPP1i/8NIIjFqGGbQ=";
        };

        cargoDeps = prev.cargoDeps.overrideAttrs (previousAttrs: {
          vendorStaging = previousAttrs.vendorStaging.overrideAttrs {
            inherit (final) src;
            outputHash = "sha256-+73aI/jt5mu6dR6PR9Q08hPdOsWukb/z9crIdMMeF7U=";
          };
        });
      }
    ))
  ];

  postPatch = ''
    mkdir -p generated/cli

    # we set HOME to a writable directory to avoid cache dir permission issues
    export HOME=$TMPDIR

    ${lib.concatMapStringsSep "\n" (
      name:
      let
        noPythonPackage = "${name}-nopython";
      in
      "${rustPackages.${noPythonPackage}}/bin/${name} print-all-help --markdown > generated/cli/${
        lib.replaceStrings [ "-" ] [ "-" ] name
      }.md"
    ) binaryPackageNames}

    cp ${../secrets.nix} generated/secrets.nix
  '';

  buildPhase = "mdbook build";

  installPhase = ''
    mkdir -p $out
    cp -r book/html/* $out/
  '';
}
