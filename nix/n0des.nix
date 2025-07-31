{
  lib,
  stdenv,
  fetchurl,
  autoPatchelfHook,
  makeWrapper,
}:

let
  tag = "latest";
  target = "linux-amd64";

  sha256 = "sha256-Lcjwb1OYELChqtNxZpOj3/nqDUcUt0tNVoYCeAwEc1s=";
in

stdenv.mkDerivation rec {
  pname = "n0des";
  version = if tag == "latest" then "unstable" else tag;

  src = fetchurl {
    url = "https://vorc.s3.us-east-2.amazonaws.com/n0des-${target}-${tag}";
    sha256 = sha256;
  };

  dontUnpack = true;

  nativeBuildInputs = [
    autoPatchelfHook
    makeWrapper
  ];

  installPhase = ''
    runHook preInstall

    mkdir -p $out/bin

    cp $src $out/bin/n0des
    chmod +x $out/bin/n0des

    runHook postInstall
  '';
  meta = with lib; {
    description = "n0des binary";
    homepage = "https://github.com/n0-computer/n0des";
    license = with licenses; [
      mit
      asl20
    ];
    maintainers = [ ];
    platforms = [ "x86_64-linux" ];
    mainProgram = "n0des";
  };
}
