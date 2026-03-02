# Secrets

We manage secrets in our repo using `agenix`.
These secrets are keyed to specific developers via SSH public keys.
Some are used for deployments, and some can be used for development.

You can read more about agenix and how secrets are used in our deployment [HERE](https://garnix.io/docs/hosting/secrets)

## What secrets do we store?

```nix
{{#include ../../generated/secrets.nix}}
```

## Editing a secret

you must have your pubkey listed in `secrets.nix` for a secret if you want to modify the existing one!

ask someone whose key is in `secrets.nix` to be added.

To edit the secret `whatever.age`, run

```bash
agenix -e secrets/whatever.age
```
