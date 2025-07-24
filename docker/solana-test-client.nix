{
  pkgs,
  nixglhostRustPackages,
  inputs,
  psycheLib,
}:
let
  solana = inputs.solana-pkgs.packages.${pkgs.system}.default;
in
pkgs.dockerTools.streamLayeredImage {
  name = "psyche-solana-test-client";
  tag = "latest";

  contents = with pkgs; [
    solana
    bashInteractive
    busybox
    cacert
    nixglhostRustPackages.psyche-solana-client-nixglhost

    # Create proper system structure including /tmp
    (pkgs.runCommand "system-setup" { } ''
      mkdir -p $out/etc $out/tmp $out/var/tmp $out/run

      # Create basic passwd and group files
      cat > $out/etc/passwd << EOF
        root:x:0:0:root:/root:/bin/bash
        nobody:x:65534:65534:nobody:/nonexistent:/bin/false
        EOF

      cat > $out/etc/group << EOF
        root:x:0:
        nobody:x:65534:
        EOF

      # Set proper permissions for temp directories
      chmod 1777 $out/tmp
      chmod 1777 $out/var/tmp
      chmod 755 $out/run
    '')

    (pkgs.runCommand "entrypoint" { } ''
      mkdir -p $out/bin
      cp ${./test/client_test_entrypoint.sh} $out/bin/client_test_entrypoint.sh
      cp ${./test/run_owner_entrypoint.sh} $out/bin/run_owner_entrypoint.sh
      chmod +x $out/bin/client_test_entrypoint.sh
      chmod +x $out/bin/run_owner_entrypoint.sh
    '')
  ];

  config = {
    Env = [
      "NVIDIA_DRIVER_CAPABILITIES=compute,utility"
      "NVIDIA_VISIBLE_DEVICES=all"
    ];
    Entrypoint = [ "/bin/client_test_entrypoint.sh" ];
  };
}
