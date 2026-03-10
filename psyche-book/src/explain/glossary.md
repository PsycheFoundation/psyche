# Psyche Glossary

**ActiveStep**
The state machine phases a `Client` goes through during a training `Round` or `Epoch`, synchronized with the `Coordinator`'s `RunState`. Includes `Warmup`, `Training`, `Witness`, and `Cooldown`.

**AMD ROCm**
An alternative GPU compute platform to NVIDIA's CUDA. Support for ROCm is planned for Psyche clients in the future.

**Authorizer**
A Solana program that issues authorizations to specific users.

**Authorization**
A specific role (scope) assigned to a single user (grantee) by a specific authority (grantor). The grantee can then delegate authorization to other keys (the delegates) that can act on its behalf. In practice, this is useful for managing permissions to nodes in data center clusters easily.

**Batch**
A subset of the training data processed by clients in a single step within a `Round`. Identified by a `BatchId`.

**BatchId**
A unique identifier for a specific `Batch` of training data.

**Bloom Filter**
A probabilistic data structure used for efficient set membership testing (e.g., checking if a client's commitment has been witnessed). Used in `WitnessBloom`. Has a small chance of false positives.

**BLOOM_FALSE_RATE**
The target false positive rate (1% in this case) for the `Bloom Filters` used in the witness protocol.

**Checkpoint**
A saved state of the LLM being trained. Psyche uses checkpoints to allow runs to be paused, resumed, or recovered after interruptions. Checkpoints can be stored in a central `HubRepo` or shared between clients via `P2P`.

**Checkpointers**
Designated, trusted participants responsible for saving the model `Checkpoint` during the `Cooldown` phase.

**Client**
The software participants run on their own hardware (typically with a GPU) to contribute to the distributed training process. Clients perform computations, submit results (`Commitments`), and participate in `Witnessing`.

**ClientState**
The status of a `Client` as tracked by the `Coordinator`. Key states include `Healthy`, `Dropped`, `Withdrawn`, and `Ejected`.

**Commitment**
A cryptographic hash (SHA-256) of a client's computational results for a given `Batch`. Submitting commitments allows the `Coordinator` and `Witnesses` to verify work was done without transferring the full results initially.

**Committee**
The particular role of a client in a given round. Can be one of `Trainer`, `Verifier` or `TieBreaker`.

**Cooldown**
A phase (`RunState` and `ActiveStep`) at the end of an `Epoch` where model `Checkpoints` are saved and the system prepares for the next epoch.

**Coordinator**
The central orchestrator of the Psyche training system, implemented as a Solana program. It manages the training life cycle (`RunState`), client participation (`ClientState`), data batch assignment, and `Witnessing`.

**CoordinatorConfig**
The set of parameters defining how a specific training run operates (e.g., `warmup_time`, `witness_quorum`, `rounds_per_epoch`).

**CUDA**
NVIDIA's parallel computing platform and programming model, required for running the Psyche client on NVIDIA GPUs.

**Data Provider**
Component responsible for supplying the training data in organized `Batches`.

**Desync**
An error state (`StepError::Desync`) occurring when a `Client`'s `ActiveStep` falls out of synchronization with the `Coordinator`'s `RunState`.

**Docker**
A platform used to build, ship, and run applications in `Containers`. Psyche uses Docker to distribute and run the client software.

**Dropped**
A `ClientState` indicating a client has become unresponsive or disconnected unexpectedly.

**Ejected**
A `ClientState` indicating a client has been forcibly removed from the training run, typically due to failing health checks or malicious behavior. Ejected clients may be subject to `Slashing`.

**Epoch**
A major cycle in the training process, composed of multiple `Rounds`. A `Checkpoint` starts with the `WaitingForMembers` and `Warmup` phases and ends with a `Cooldown` phase.

**Exited Clients**
A buffer on the `Coordinator` holding records of clients that have recently left the run (`Dropped`, `Withdrawn`, `Ejected`).

**Finished**
A `RunState` indicating that the training run has completed its configured `total_steps`.

**Garnix**
CI (Continuous Integration) service based on `Nix`, used by `Psyche`.

**Health Check**
A verification procedure (`health_check()`) initiated by designated `witness` clients. Its purpose is to monitor peer clients and confirm they are actively processing their assigned training batches. When a witness client detects a peer that appears unresponsive or failing (`unhealthy`), it notifies the central coordinator. The coordinator independently verifies the status of the reported peer by running its own health check. If this verification is verified then the peer is marked as `unhealthy` and is kicked.

**Healthy**
The desired `ClientState`, indicating the client is connected, responsive, and participating correctly in the training process. Only Healthy clients typically receive `Rewards`.

**HubRepo**
A centralized repository location (e.g., Hugging Face, S3 bucket) where the model `Checkpoint` can be stored, particularly when initializing or if P2P storage is unavailable.

**Iroh**
A `P2P` library that `Psyche` uses for data-sharing between the clients.

**Lightweight Hashing**
Using efficient hashing algorithms like SHA-256 for `Commitments` to allow for fast verification by the `Coordinator` and `Witnesses`.

**Metal**
Apple's graphics and compute API. A future backend target for running the Psyche client on Mac hardware.

**min_clients**
The minimum number of `Healthy` clients required for a training run to progress beyond the `WaitingForMembers` state.

**Mining Pool**
A Solana program that implements a basic "mining" or lending pool mechanism where users (lenders) can deposit collateral into a pool to delegate funds to other participants with more compute power and eventually claim redeemable tokens proportionate to their share of the total deposited collateral.

**NUM_STORED_ROUNDS**
A constant defining how many past rounds' states are kept in the `Coordinator`'s history buffer (e.g., 4 rounds).

**Nix**
Tool for declarative and reproducible builds used by `Psyche`.

**Opportunistic Witnessing**
A feature that allows progressing early from the `RoundTrain` phase to the `Witness` phase, given that the `witness quorum` is reached.

**Paused**
A `RunState` where the training process is temporarily stopped by manual intervention. Can be resumed later.

**P2P**
Peer-to-Peer, meaning a client acts both as a client and as a server, sharing data with it's peers. This is the intended way of data-sharing during a stable run.

**Psyche**
Nous Research's set of systems that enable distributed training of transformer-based AI models over the internet.

**Round**
A smaller cycle within an `Epoch`. Involves a training phase (`RoundTrain`) and a validation phase (`RoundWitness`).

**RoundTrain**
The phase (`RunState` and `ActiveStep`) where clients download assigned data `Batches`, perform training computations (e.g., calculate gradients), and submit `Commitments`.

**RoundWitness**
The phase (`RunState` and `ActiveStep`) where clients act as `Witnesses` to validate the `Commitments` submitted by other clients during `RoundTrain`. Requires a `witness_quorum` to succeed.

**rounds_per_epoch**
A configuration parameter (`CoordinatorConfig`) specifying how many `Rounds` make up one `Epoch`.

**RunState**
The overall state of the training run as managed by the `Coordinator`. Examples include `Uninitialized`, `WaitingForMembers`, `Warmup`, `RoundTrain`, `RoundWitness`, `Cooldown`, `Paused`, `Finished`.

**SHA-256**
The specific cryptographic hash function used to create `Commitments` in Psyche.

**Solana**
The blockchain platform on which the Psyche `Coordinator` program runs.

**StepError**
A category of errors related to the `Client`'s `ActiveStep` progression, such as `Desync`.

**tick()**
A function periodically called on the `Coordinator` program to drive the state machine transitions (advancing `RunState` based on time limits, client counts, and submitted results). Specific versions exist for different states (e.g., `tick_waiting_for_members`, `tick_round_witness`).

**total_steps**
A configuration parameter defining the total number of training steps or batches the run aims to complete before entering the `Finished` state.

**Training**
The `ActiveStep` where the client actively computes gradients or other training operations on its assigned data `Batch`.

**Treasurer**
A Solana program that runs on top of psyche's Coordinator managing the distribution of rewards to the clients and keeping track of the points earned by each client in the training process.

**Uninitialized**
The default starting `RunState` of the `Coordinator` before a training run is configured and started.

**WaitingForMembers**
The `RunState` where the `Coordinator` waits for the minimum number of clients (`min_clients`) to connect and become `Healthy` before starting the training process.

**Warmup**
The initial phase (`RunState` and `ActiveStep`) of a training run where clients download the model `Checkpoint` and initialize their training environment.

**Witness**
A `Client` selected to validate other client's work.

**WitnessBloom**
The specific `Bloom Filter` used on the `Coordinator` to track which client `Commitments` have been successfully witnessed.

**Witness Quorum**
The minimum number of clients that must successfully act as `Witnesses` and agree on the validity of results for a `Round` to be considered successful.

**Withdrawn**
A `ClientState` indicating that a client has exited the run.
