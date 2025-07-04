{
  psyche-website-wasm,
  psycheLib,
  solana-coordinator-idl,
  solana-mining-pool-idl,
}:
psycheLib.mkWebsitePackage {
  package = "shared";

  preBuild = ''
    mkdir -p shared/idl/
    pushd shared/idl/
      cp ${solana-coordinator-idl}/idl.json ./coordinator_idl.json
      cp ${solana-coordinator-idl}/idlType.ts ./coordinator_idlType.ts
      cp ${solana-mining-pool-idl}/idl.json ./mining-pool_idl.json
      cp ${solana-mining-pool-idl}/idlType.ts ./mining-pool_idlType.ts
    popd

    mkdir -p wasm/dist
    cp -r ${psyche-website-wasm}/* wasm/pkg
  '';

  installPhase = ''
    runHook preInstall
    mkdir -p $out
    cp -r shared $out/
    runHook postInstall
  '';
}
