# this file contains secrets that we can store encrypted in this repo.
# they can be decrypted by the specified ssh public keys using `agenix`.
let
  keys = import ./nix/keys.nix;
in
{
  ## Local Development
  # a shared devnet wallet
  "secrets/devnet/wallet.age".publicKeys = keys.allDevKeys;

  # RPC url for devnet
  "secrets/devnet/rpc.age".publicKeys = keys.allDevKeys;

  # RPC url for mainnet
  "secrets/mainnet/rpc.age".publicKeys = keys.allDevKeys;

  ## Deployments

  # all RPC urls for our old, soon-to-be-deprecated devnet indexer
  "secrets/devnet/backend-old.age".publicKeys = keys.allKeys;

  # all RPC urls for our mainnet indexer
  "secrets/mainnet/backend-old.age".publicKeys = keys.allKeys;
}
