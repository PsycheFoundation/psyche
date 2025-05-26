# this file lists SSH keys of developers of this repo.
# some of these are used to ssh into development machines,
# some are used to decrypt secrets.
# which key -> which secret is determined in secrets.nix.
rec {
  # from garnix, a unique ssh key for NousResearch/psyche
  nousRepoKey = "age1qjv2zfrgwdhc0pw3t2rqk0zv6zudwqjjdz8plmy9u2edyxm77crqr73uqj";

  # from garnix, a unique ssh key for PsycheFoundation/psyche
  psycheRepoKey = "age13lxrsre5klvs777t6c748tnuskjx9g54wffc6at3hfhx2fj4ccts5ale7t";

  ariLunaKey = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIIL+5IDeIKvYpQllVsU/soRu27KyPTA5FXvZM5Z8+ms7 arilotter@gmail.com";
  ariHermesKey = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIMKaPWTrDp1sp3NUXiM/JXKfivQQ6TLxMy7Fyaq59L7y arilotter@gmail.com";
  ariKeys = [
    ariHermesKey
    ariLunaKey
  ];

  allDevKeys = ariKeys;

  allKeys = [
    nousRepoKey
    psycheRepoKey
  ] ++ allDevKeys;
}
