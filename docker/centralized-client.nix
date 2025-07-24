{
  pkgs,
  nixglhostRustPackages,
  inputs,
  psycheLib,
}:
pkgs.dockerTools.streamLayeredImage {
  name = "psyche-centralized-client";
  tag = "latest";

  # Copy the binary and the entrypoint script into the image
  contents = [
    pkgs.bashInteractive
    nixglhostRustPackages.psyche-centralized-client-nixglhost
  ];

  config = {
    Env = [
      "NVIDIA_DRIVER_CAPABILITIES=compute,utility"
      "NVIDIA_VISIBLE_DEVICES=all"
    ];
  };
}
