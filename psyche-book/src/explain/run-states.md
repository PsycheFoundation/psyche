# Run States

During a training epoch, the Coordinator progresses through several distinct states. Each state has specific responsibilities and transitions to the next state based on time-based conditions or specific events.

## The Four States

1. **[Warmup Phase](./warmup-phase.md)** - Clients download the model and load it onto their GPUs
2. **[Train Phase](./train-phase.md)** - Clients perform training on assigned data batches and exchange results
3. **[Witness Phase](./witness-phase.md)** - Witnesses verify and submit proofs of completed training work
4. **[Cooldown Phase](./cooldown-phase.md)** - The epoch concludes and checkpoints are saved

## State Flow

The typical flow through these states is:

```
WaitingForMembers → Warmup → [Train → Witness] × N rounds → Cooldown → WaitingForMembers
```

Where N is determined by the `rounds_per_epoch` configuration setting.

For a detailed overview of how these states fit into the broader system architecture, see the [Workflow Overview](./workflow-overview.md).
