{
  pkgs,
  miningPoolRpc,
  miningPoolCluster,
  coordinatorCluster,
  backendPath,
  ...
}:
let
  psyche-website-wasm = pkgs.callPackage ../wasm { };
  psyche-website-shared = pkgs.callPackage ../shared { };
  mkWebsitePackage = pkgs.callPackage ../common.nix { };
in
mkWebsitePackage {
  package = "frontend";

  preBuild = ''
    mkdir -p wasm/dist
    cp -r ${psyche-website-wasm}/* wasm/pkg

    mkdir -p shared
    cp -r ${psyche-website-shared}/shared/* shared/

    export VITE_MINING_POOL_RPC=${miningPoolRpc} 
    export VITE_BACKEND_PATH=${backendPath}
    export VITE_MINING_POOL_CLUSTER=${miningPoolCluster}
    export VITE_COORDINATOR_CLUSTER=${coordinatorCluster}
  '';

  installPhase = ''
    runHook preInstall

    mkdir -p $out
    cp -r frontend/dist/* $out/

    runHook postInstall
  '';
}
