# Authentication and Keys

When clients participate in a decentralized training run, a set of Solana Keypairs is used to authenticate each type of user.

## Users Roles

A different set of key will be used for each role within the Training flow.

The following roles will be important:

- The Run's `main_authority` is the private-key that creates and owns the run, it is the only key that is allowed to modify the run's configuration.

- The Run's `join_authority` is the private-key that is responsible for allowing or disallowing clients's keys to join a training run. It is set by the `main_authority` during the creation of the Run.

- A client's `authorizer` key is the user master key (for compute providers). That can then set delegate keys that can join the run on its behalf.

- A Client's `delegate` key is a temporary and ephemeral key that can be allowed to join a run's training on behalf of a user's master key.

A Training run can be configured to be restricted to only a set of whitelisted keys, this kind of run is considered "Permissioned". As opposed to a "Permissionless" which is open to anyone without per-user `authorization` required.

## Permissionless Runs

Permissionless runs are open to anyone without any `authorization` required. The owner of the run can set this for a run when creating it. This type of authorization can be made by creating an `authorization` with a special `authorizer` valid for everyone: `11111111111111111111111111111111`

A CLI is provided for this:

```sh
run-manager join-authorization-create \
    --rpc [RPC] \
    --wallet-private-key-path [JOIN_AUTHORITY_KEYPAIR_FILE] \
    --authorizer 11111111111111111111111111111111
```

## Permissioned Runs

In order to be able to join a permissioned run, a user must first be whitelisted through a dedicated `authorization`.

This is done through the following steps:

1. The `join_authority` issues an `authorization` for an `authorizer` (the user master key)
2. The `authorizer` (the user master key) sets a list of `delegate` keys that can join the run on its behalf
3. The `delegate` (an user temporary key) then can join a run

## Keys Management

For the `join_authority` to issues new `authorization`, a CLI is provided:

```sh
run-manager join-authorization-create \
    --rpc [RPC] \
    --wallet-private-key-path [JOIN_AUTHORITY_KEYPAIR_FILE] \
    --authorizer [USER_MASTER_PUBKEY]
```

For the `authorizer` to then set a list of delegate, the following CLI is provided:

```sh
run-manager join-authorization-delegate \
    --rpc [RPC] \
    --wallet-private-key-path [USER_MASTER_KEYPAIR_FILE] \
    --join-authority [JOIN_AUTHORITY_PUBKEY]
    --delegates-clear [true/false] # Optionally remove previously set delegates
    --delegates-added [USER_DELEGATES_PUBKEYS] # Multiple pubkeys can be added
```

Removing the authorization is also possible through CLI:

```sh
run-manager join-authorization-delete \
    --rpc [RPC] \
    --wallet-private-key-path [JOIN_AUTHORITY_KEYPAIR_FILE] \
    --authorizer [USER_MASTER_PUBKEY]
```

## Further information

To see how the authorization creation for a real run fits in the configuration see the [Authorization section](./create-run.md#Setting-up-Join-Authorizations) in the create run guide.

The source code for the `authorizer` smart contract used by the Psyche's coordinator can be found here with its readme: <https://github.com/PsycheFoundation/psyche/tree/main/architectures/decentralized/solana-authorizer>
