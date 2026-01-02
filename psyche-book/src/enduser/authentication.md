# Authentication and Keys

When clients participate in a decentralized training run, a set of Solana Keypairs is used to authenticate each type of user.

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

For the `join_authority` (the grantor) to issues new `authorization` a CLI is provided:

```sh
psyche-solana-client join-authorization-create \
    --rpc [RPC] \
    --wallet-private-key-path [JOIN_AUTHORITY_KEYPAIR_FILE] \
    --authorizer [USER_MASTER_PUBKEY]
```

For the `authorizer` (the grantee) to set a list of delegate, the following CLI is provided:

```sh
psyche-solana-client join-authorization-delegate \
    --rpc [RPC] \
    --wallet-private-key-path [USER_MASTER_KEYPAIR_FILE] \
    --join-authority [JOIN_AUTHORITY_PUBKEY]
    --delegates-clear [true/false]
    --delegates-added [DELEGATE_PUBKEY]+
```

## Further information

The source code for the `authorizer` smart contract used by the Psyche's coordinator can be found here with its readme: <https://github.com/PsycheFoundation/psyche/tree/main/architectures/decentralized/solana-authorizer>
