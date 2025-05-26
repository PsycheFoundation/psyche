{
  self,
  inputs,
  ...
}:
{
  flake =
    let
      keys = import ./keys.nix;
      backend-port = "3000";
      base-system = [
        inputs.garnix-lib.nixosModules.garnix
        inputs.agenix.nixosModules.default

        (
          { ... }:
          {
            # required minimal setup for garnix
            garnix.server.enable = true;

            # allow decrypting secrets
            age.identityPaths = [
              "/var/garnix/keys/repo-key"
            ];

            # assume we're doing webserver work
            networking.firewall.allowedTCPPorts = [
              80
              443
            ];

            # custom pkgs overlays
            nixpkgs = import ./pkgs.nix {
              inherit inputs;
              gitcommit = self.rev or self.dirtyRev;
            };

            system.stateVersion = "25.05";
          }
        )
      ];

      # lets devs SSH into these boxes. probably don't enable in production for safety.
      debug-ssh =
        { ... }:
        {
          services.openssh = {
            enable = true;
            settings = {
              PasswordAuthentication = false;
              KbdInteractiveAuthentication = false;
            };
          };
          users.users."nous" = {
            isNormalUser = true;
            home = "/home/nous";
            description = "debug ssh user";
            extraGroups = [ "wheel" ];
            openssh.authorizedKeys.keys = keys.allKeys;
          };
          security.sudo.wheelNeedsPassword = false;
        };

      psyche-website-backend =
        secret-file:
        {
          config,
          pkgs,
          ...
        }:
        let
          psyche-website-backend = pkgs.callPackage ../website/backend { };
        in
        {
          age.secrets.backendRpc = {
            file = secret-file;
            owner = "root";
            group = "root";
          };

          systemd.services.backend = {
            description = "psyche website backend";
            wantedBy = [ "multi-user.target" ];
            wants = [ "network-online.target" ];
            after = [ "network-online.target" ];
            environment = {
              PORT = backend-port;
              # the RPC urls, alongside the contract addrs, are defined in the EnvironmentFile since they contain secrets.
            };
            serviceConfig = {
              Type = "simple";
              StateDirectory = "backend";
              DynamicUser = true;
              EnvironmentFile = config.age.secrets.backendRpc.path;
              # don't start until we have DNS!
              ExecStartPre = "/bin/sh -c 'until ${pkgs.bind.host}/bin/host example.com; do sleep 1; done'";
              ExecStart = pkgs.lib.getExe psyche-website-backend;
            };
          };
        };

      persistentPsycheWebsite =
        {
          configName,
          backendSecret,
          miningPoolRpc,
          coordinatorCluster,
          miningPoolCluster,
          hostnames ? [ ],
        }:
        inputs.nixpkgs.lib.nixosSystem {
          system = "x86_64-linux";
          modules = base-system ++ [
            debug-ssh
            (psyche-website-backend backendSecret)
            (
              {
                pkgs,
                ...
              }:
              let
                backendPath = "/api";
                psyche-website-frontend = pkgs.callPackage ../website/frontend {
                  inherit
                    miningPoolRpc
                    backendPath
                    coordinatorCluster
                    miningPoolCluster
                    ;
                };
              in
              {
                garnix.server.persistence = {
                  enable = true;
                  name = configName;
                };
                services.caddy =
                  let
                    cfg = ''
                      handle {
                        root * ${psyche-website-frontend}
                        ${serveStaticSpa}
                      }

                      handle_path ${backendPath}/* {
                        reverse_proxy :${backend-port}
                      }
                    '';
                  in
                  {
                    enable = true;
                    virtualHosts =
                      {
                        "http://${configName}.*.psyche.nousresearch.garnix.me".extraConfig = cfg;
                        "http://${configName}.*.psyche.psychefoundation.garnix.me".extraConfig = cfg;
                      }
                      // (builtins.listToAttrs (
                        map (hostname: {
                          name = hostname;
                          value = {
                            extraConfig = cfg;
                          };
                        }) hostnames
                      ));
                  };
              }
            )
          ];
        };

      mainnetFrontendRpc = "https://quentin-uzfsvh-fast-mainnet.helius-rpc.com";
      devnetFrontendRpc = "https://bree-dtgg3j-fast-devnet.helius-rpc.com";

      serveStaticSpa = ''
        # we want index.html to have no cache, since it's tiny
        # but all other files to be cached.
        # since there's multiple paths that serve index.html,
        # we only cache files that "exist", and skip caching
        # on files that don't (and index.html)

        @index_html path /index.html
        header @index_html Cache-Control "no-cache, no-store, must-revalidate"
        header @index_html Pragma "no-cache" 
        header @index_html Expires "0"


        @existing_non_index {
            file {path}
            not path /index.html
        }
        file_server @existing_non_index

        @spa_routes {
          not file {path}
        }

        header @spa_routes Cache-Control "no-cache, no-store, must-revalidate"
        header @spa_routes Pragma "no-cache" 
        header @spa_routes Expires "0"
        rewrite @spa_routes /index.html

        file_server
      '';
    in
    {
      # server for hosting the frontend/backend, for testing
      nixosConfigurations."psyche-http-devnet" = persistentPsycheWebsite {
        configName = "psyche-http-devnet";
        hostnames = [ "devnet-preview.psyche.network" ];
        backendSecret = ../secrets/devnet/backend.age;
        miningPoolRpc = devnetFrontendRpc;
        coordinatorCluster = "devnet";
        miningPoolCluster = "devnet";
      };
      nixosConfigurations."psyche-http-mainnet" = persistentPsycheWebsite {
        configName = "psyche-http-mainnet";
        hostnames = [ "mainnet-preview.psyche.network" ];
        backendSecret = ../secrets/mainnet/backend.age;
        miningPoolRpc = mainnetFrontendRpc;
        coordinatorCluster = "devnet";
        miningPoolCluster = "mainnet";
      };

      # server for hosting the mainnet docs & frontend/backend.
      nixosConfigurations."psyche-http" = inputs.nixpkgs.lib.nixosSystem {
        system = "x86_64-linux";
        modules = base-system ++ [
          debug-ssh
          (psyche-website-backend ../secrets/mainnet/backend.age)
          (
            {
              pkgs,
              ...
            }:
            let
              backendPath = "/api";
              psyche-website-frontend = pkgs.callPackage ../website/frontend {
                miningPoolRpc = mainnetFrontendRpc;
                inherit backendPath;
                coordinatorCluster = "devnet";
                miningPoolCluster = "mainnet";
              };
            in
            {
              # don't lose our nice db every time we deploy!
              garnix.server.persistence = {
                enable = true;
                name = "psyche-mainnet";
              };

              services.caddy = {
                enable = true;
                virtualHosts =
                  let
                    psyche-website = ''
                      handle {
                        root * ${psyche-website-frontend}
                        ${serveStaticSpa}
                      }

                      handle_path ${backendPath}/* {
                        reverse_proxy :${backend-port}
                      }
                    '';
                  in
                  {
                    "https://docs.psyche.network".extraConfig = ''
                      root * ${self.packages.${pkgs.system}.psyche-book}
                      file_server
                    '';

                    "https://psyche.network".extraConfig = psyche-website;
                    "http://psyche-http.main.psyche.nousresearch.garnix.me".extraConfig = psyche-website;
                    "http://psyche-http.main.psyche.psychefoundation.garnix.me".extraConfig = psyche-website;
                  };
              };
            }
          )
        ];
      };

      # server for hosting docs, for test deploys
      nixosConfigurations."psyche-http-docs" = inputs.nixpkgs.lib.nixosSystem {
        system = "x86_64-linux";
        modules = base-system ++ [
          (
            { pkgs, ... }:
            {
              services.caddy =
                let
                  conf = ''
                    root * ${self.packages.${pkgs.system}.psyche-book}
                    file_server
                  '';
                in
                {
                  enable = true;
                  virtualHosts = {
                    "http://psyche-http-docs.test-deploy-docs.psyche.nousresearch.garnix.me".extraConfig = conf;
                    "http://psyche-http-docs.test-deploy-docs.psyche.psychefoundation.garnix.me".extraConfig = conf;
                    "http://docs-preview.psyche.network".extraConfig = conf;
                  };
                };
            }
          )
        ];
      };
    };

  perSystem =
    {
      pkgs,
      system,
      ...
    }:
    {
      checks =
        let
          nixosConfigurations = self.nixosConfigurations;
          configNames = builtins.attrNames nixosConfigurations;

          # Helper function to create a check for a specific configuration
          validateCaddyfile =
            configName:
            let
              config = nixosConfigurations.${configName};
              caddyConfig = config.config.services.caddy.configFile;
            in
            {
              name = "validate-${configName}-caddyfile";
              value =
                pkgs.runCommand "validate-${configName}-caddyfile"
                  {
                    nativeBuildInputs = with pkgs; [
                      bubblewrap
                      caddy
                    ];
                  }
                  ''
                    # the file must be named Caddyfile to have it validated.
                    cp ${caddyConfig} ./Caddyfile

                    # use bubblewrap to remap /var that caddy wants to write logs to
                    bwrap \
                      --ro-bind /nix /nix \
                      --dir /var \
                      --bind . /tmp/work \
                      --chdir /tmp/work \
                      caddy validate --config ./Caddyfile

                    # create an empty file to mark success
                    touch $out
                  '';
            };
        in
        # check caddyfiles in each nixos configuration
        builtins.listToAttrs (
          builtins.map validateCaddyfile (
            builtins.filter (
              name: nixosConfigurations.${name}.config.services.caddy.enable or false
            ) configNames
          )
        );
    };
}
