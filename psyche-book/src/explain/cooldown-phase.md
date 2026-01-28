# Cooldown Phase

## Cooldown phase (state: Cooldown)

The _Cooldown_ phase is the last phase of an epoch, during which the Coordinator waits for either the _Cooldown_ period to elapse, or a checkpoint to have happened.

When the _Cooldown_ phase begins, the Coordinator resets the current model checkpoint state to `Checkpoint::P2P`, signifying that new joiners should download the latest copy of the model from the other participants.

Upon exiting the _Cooldown_ phase, the Coordinator transitions to the next epoch, saving the previous epoch state, and moving back to the _WaitingForMembers_ phase.
