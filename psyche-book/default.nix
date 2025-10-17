{
  lib,
  mdbook,
  fetchFromGitHub,
  rustPlatform,
  stdenvNoCC,
  mdbook-mermaid,
  mdbook-linkcheck,

  # custom args
  rustPackages,
  rustPackageNames,
}:
let
  mdbook-0-4-47 = mdbook.overrideAttrs (
    oldAttrs:
    let
      version = "0.4.47";
      src = fetchFromGitHub {
        owner = "rust-lang";
        repo = "mdBook";
        tag = "v${version}";
        hash = "sha256-XTvC2pGRVat0kOybNb9TziG32wDVexnFx2ahmpUFmaA=";
      };
    in
    {
      inherit version src;
      cargoDeps = rustPlatform.fetchCargoVendor {
        inherit (oldAttrs) pname;
        inherit version src;
        allowGitDependencies = false;
        hash = "sha256-ASPRBAB+elJuyXpPQBm3WI97wD3mjoO1hw0fNHc+KAw=";
      };
    }
  );
in
stdenvNoCC.mkDerivation {
  __structuredAttrs = true;

  name = "psyche-book";
  src = ./.;

  nativeBuildInputs = [
    mdbook-0-4-47
    mdbook-mermaid
    mdbook-linkcheck
  ];

  postPatch = ''
    mkdir -p generated/cli

    ${lib.concatMapStringsSep "\n" (
      name:
      "${rustPackages.${name}}/bin/${name} print-all-help --markdown > generated/cli/${
        lib.replaceStrings [ "-" ] [ "-" ] name
      }.md"
    ) rustPackageNames}

    cp ${../secrets.nix} generated/secrets.nix
  '';

  buildPhase = "mdbook build";

  installPhase = ''
    mkdir -p $out
    cp -r book/html/* $out/
  '';
}
