# Psyche Solana Treasurer

This smart contract provides an Incentive layer on top of the Psyche's
coordinator program.

## How it works

The `Treasurer.Run` PDA can be created by an authority using the `run_create`
IX.

Creating the `Treasurer.Run` PDA will automatically create a training
`Coordinator.CoordinatorInstance` and `Coordinator.CoordinatorAccount` owned by
the `Treasurer.Run` smart contract's PDA, it will use the
`Coordinator.init_coordinator` IX.

The underlying `Coordinator.CoordinatorAccount` can then be configured
indirectly, through using the `Treasurer.run_update` for all permissioned
`Coordinator` IXs.

The underlying `CoordinatorAccount` permissionless IXs such as `join` can be
used by simply using the `Coordinator` IXs directly, without using the
`Treasurer` at all.

A set of reward tokens can then be deposited inside of the `Treasurer.Run`'s ATA
for fair distribution during the training of the underlying coordinator run.

Once a client has earned compute points in the underlying `Coordinator`'s run,
that same client can then claim to have participated in the run by creating a
`Treasurer.Participant` PDA.

Once that client has earned enough points and once the reward token has been
deposited into the `Treasury.Run`'s ATA, the user can the directly withdraw the
reward tokens to its wallet using the `participant_claim` IX.

The incentive reward rate can be configured in the
`Coordinator.CoordinatorAccount` itself (epoch rewards rates).

## Solana Instructions

To achieve this the smart contract provides the following capabilities:

- `run_create`, Create a normal `Run` owned by the `Treasurer`
- `run_update`, Configure the underlying `Run`'s Psyche coordinator
- `participant_create`, Must be called before a user can claim reward tokens
- `participant_claim`, Once a user earned points on the `Run`'s coordinator, the
  user can withdraw the reward tokens proportional share of the `Run`'s treasury
