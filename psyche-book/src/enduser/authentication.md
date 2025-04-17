# Authentication and Keys

When clients participate to a decentralized training run, a set of solana Keypairs is used to authenticate each type of user.

## Users Roles

A different set of key will be used for each role within the Training flow.

The following roles will be important:

- The Run's `main_authority` is the private-key that creates and owns the run, it is the only key that is allowed to modify the run's configuration.

- The Run's `join_authority` is the private-key that is responsible for allowing or disallowing clients's keys to join a training run. It is set by the `main_authority` during the creation of the Run.

- A client's `authorizer` (or grantee) key is the "master" private-key of a compute provider. That key may be allowed to join a run and to set delegate keys that can also join the run on its behalf.

- A Client's `delegate` key is a temporary and ephemeral key that can be allowed to join a run's training on behalf of a user.

A Training run can be configured to be restricted to only a set of whitelisted keys, this kind of run is considered "Permissioned". As opposed to a "Permissionless" which is open to anyone without any `authorization` required.

## Permissioned Runs

When a In order to be able to join a run, a user (with a key) must first be allowed to join a run.

This is done through the following steps:

1. The `join_authority` (the grantor) issues an `authorization` to an `authorizer` (the grantee)
2. The `authorizer` (the grantee) sets a list of `delegate` keys that can join the run on its behalf
3. The `delegate` key then can join a run

## Keys Authorizations

Make sure to install the scripting dependencies:

```bash
sudo apt-get install jq
cargo install solana_toolbox_cli
```

For the `join_authority` (the grantor) to issues new `authorization` a script is provided:

```sh
# We assume that "grantor.json" contains the Private Key of the "join_authority"
# The "grantor.json" can be created using: $ solana-keygen new -o grantee.json
# We assume that $GRANTEE_PUBKEY is set to the public key of the "authorizer" (or grantee)
# The $GRANTEE_PUBKEY can be retrieved by using: $ solana-keygen pubkey grantee.json
sh scripts/join-authorization-create.sh devnet grantor.json $GRANTEE_PUBKEY
```

For the `authorizer` (the grantee) to set a list of delegate, the following script is provided:

```sh
# We assume that $GRANTOR_PUBKEY is set to the public key of the "join_authority" of the run
# The $GRANTOR_PUBKEY can be retrieved by using: $ solana-keygen pubkey grantor.json
# We assume that "grantee.json" contains the Private Key of the "authorizer"
# The "grantee.json" can be created using: $ solana-keygen new -o grantee.json
# We assume that a set of keypairs exist at path: delegate1.json, delegate2.json, etc
sh scripts/join-authorization-set-delegates.sh devnet $GRANTOR_PUBKEY grantee.json delegate*.json
```

## Further information

The source code for the `authorizer` smart contract used by the Psyche's coordinator can be found here with its readme:

<https://github.com/NousResearch/psyche/tree/main/architectures/decentralized/solana-authorizer>