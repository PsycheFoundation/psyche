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
let
  # Pull crate binary package names from rustPackages
  # prefer -nopython suffix if available, otherwise use normal version
  allPackageNames = builtins.attrNames rustPackages;

  binaryPackageNames = lib.unique (
    builtins.filter (
      name:
      if lib.hasSuffix "-nopython" name then true else !(builtins.elem "${name}-nopython" allPackageNames)
    ) allPackageNames
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

    # we set HOME to a writable directory to avoid cache dir permission issues
    export HOME=$TMPDIR

    ${lib.concatMapStringsSep "\n" (
      name:
      let
        basename = lib.replaceStrings [ "-nopython" ] [ "" ] name;
      in
      "${rustPackages.${name}}/bin/${basename} print-all-help --markdown > generated/cli/${basename}.md"
    ) binaryPackageNames}

    cp ${../secrets.nix} generated/secrets.nix
  '';

  buildPhase = "mdbook build";

  installPhase = ''
    mkdir -p $out
    cp -r book/html/* $out/
  '';
}
