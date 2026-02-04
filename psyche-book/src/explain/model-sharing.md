# Model sharing

When an **epoch** starts, all clients must have an identical model to train with.

At the beginning of a run, all clients must download the model parameters, tokenizer configuration, and model configuration from HuggingFace, where the model must have been previously uploaded or updated.

Each client will then modify their copy of the model by receiving new training results from other clients and applying them. This keeps everyone's copy of model identical within an **epoch** without an additional full synchronization step.

When a new client joins a run that has already progressed past its first epoch, it would not be correct for the client to download the original model from HuggingFace, as the model parameters would have already been updated during training. Instead, the new client must acquire a copy of the model from the peers who have been actively training it.

This synchronization process occurs during the _Warmup_ phase, while the coordinator waits to begin the next _Training_ phase.

To address this, we **checkpoint** the model at the end of an **epoch**, where clients save and share the entire model for new peers to join. There are two checkpointing variants: HuggingFace based and P2P based.

## HuggingFace checkpoint

In this approach, a client or a set of clients can optionally run as **checkpointers** if they declare a checkpoint URL when joining the run. These clients upload their copy of updated model to HuggingFace after each epoch, and send the URL for this checkpoint to the coordinator. When a new client joins the run, it retrieves the checkpoint URL from the coordinator, and connects to HuggingFace to download the latest copy of the model parameters and configuration files.

## P2P checkpoint

In the peer-to-peer (P2P) approach, a new client synchronizes by obtaining the latest model directly from other peers. It receives the model information and parameters from any available peer, requesting a set of parameters for each layer from different clients. This process allows the client to assemble the latest model state and participate in the training without an explicit upload step to a central server occurring.

Here's an example of a P2P model sharing interaction:

```mermaid
flowchart TB
   C((Coordinator))
   C1[Client 1]
   C2[Client 2]
   C3[Client 3]
   C4[Joining Client]
   C --warmup---> C1
   C --warmup---> C2
   C --warmup---> C3
   C2 --Model config--> C4
   C4 -.Join.-> C
   C1 -.Layer 1 weights.-> C4
   C2 -.Layer 2 weights.-> C4
   C3 -.Layer 3 weights.-> C4
```
