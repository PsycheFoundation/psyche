{
  lib,
  stdenvNoCC,
  mdbook,
  mdbook-mermaid,
  mdbook-linkcheck2,

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
    mdbook-linkcheck2
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
