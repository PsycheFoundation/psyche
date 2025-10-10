{ lib, ... }:
{
  perSystem =
    {
      pkgs,
      ...
    }:
    let
      makeIntegrationTestApp =
        {
          testsPackage ? "integration_tests",
          package,
        }:
        {
          type = "app";
          program =
            let
              testsBin = pkgs.psycheLib.buildRustPackageWithPsychePythonEnvironment {
                package = package;
                name = testsPackage;
                type = "test";
              };
              script = pkgs.writeShellScriptBin package ''
                set -euo pipefail
                echo "docker is ready, running integration test..."
                ls ${testsBin}/bin
                ${lib.getExe testsBin}
              '';
            in
            lib.getExe script;
        };
    in
    {
      apps = {
        decentralized-integration-test = makeIntegrationTestApp {
          package = "psyche-decentralized-testing";
        };
      };
    };
}
