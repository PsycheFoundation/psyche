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

    ${lib.concatMapStringsSep "\n"
      (
        name:
        let
          basename = lib.replaceStrings [ "-nopython" ] [ "" ] name;
        in
        "${rustPackages.${name}}/bin/${basename} print-all-help --markdown > generated/cli/${basename}.md"
      )
      [
        "psyche-centralized-local-testnet"
        "psyche-sidecar"
        "psyche-centralized-client"
        "psyche-centralized-server"
        "psyche-solana-client"
      ]
    }

    cp ${../secrets.nix} generated/secrets.nix
  '';

  buildPhase = "mdbook build";

  installPhase = ''
    mkdir -p $out
    cp -r book/html/* $out/
  '';
}
