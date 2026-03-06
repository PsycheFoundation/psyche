# this file lists SSH keys of developers of this repo.
# some of these are used to ssh into development machines,
# some are used to decrypt secrets.
# which key -> which secret is determined in secrets.nix.
rec {

  # from garnix, a unique ssh key for PsycheFoundation/nousnet.
  # this key can be obtained via `curl https://garnix.io/api/keys/PsycheFoundation/nousnet/repo-key.public`
  # it's used for decrypting secrets (rpc URLs, et al) at runtime in our indexer
  garnixRepoKey = "age13g40jg6u362ecacuqx9fmmhfmv38r5gdfemzre32xee66y84d3ksvagdmz⏎";

  # from garnix, a unique ssh key for the push-docker-solana-client action
  # https://garnix.io/api/keys/PsycheFoundation/nousnet/actions/push-docker-solana-client/key.public
  garnixActionPushDockerSolanaClient = "age1y6k2qgczu8f2wf8uxv7x3tu2tcr5jpxg0fk4zzp7um4kvlgafu8q6ftwcq";

  ariLunaKey = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIIL+5IDeIKvYpQllVsU/soRu27KyPTA5FXvZM5Z8+ms7 arilotter@gmail.com";
  ariHermesKey = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIMKaPWTrDp1sp3NUXiM/JXKfivQQ6TLxMy7Fyaq59L7y arilotter@gmail.com";
  ariKeys = [
    ariHermesKey
    ariLunaKey
  ];

  allDevKeys = ariKeys;

  allKeys = [
    garnixRepoKey
  ]
  ++ allDevKeys;
}
