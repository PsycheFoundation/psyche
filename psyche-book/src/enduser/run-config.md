# Run configuration

A training run on Psyche is described using a Run Configuration file.
It's a `.toml` file with information about the model shape, size, checkpoints, optimizer settings, run witnessing settings, and more.

There's two top-level values in a run configuration: a `config`, and a `model`.

While some examples are described below, you can find the full range of options [for the coordinator here](https://github.com/PsycheFoundation/psyche/blob/main/shared/coordinator/src/coordinator.rs) and [for the model here](https://github.com/PsycheFoundation/psyche/blob/main/shared/coordinator/src/model.rs)

## Config

Here's a sample config with some of its options documented.

```toml
[config]
# maximum time, in seconds, to let nodes download the model from a checkpoint / other nodes
warmup_time = 30

# time, in seconds, to let nodes bring the model from the GPU to disk, and to opt to join the next round.
cooldown_time = 30

# how many training rounds in one "epoch", from warmup to cooldown.
rounds_per_epoch = 20

# maximum time, in seconds, to allow nodes to train in one round.
# this will limit the types of GPUs your model can be trained on,
# since setting it low will prevent slower hardware from completing
# training in time.
max_round_train_time = 30

# time, in seconds, to allow witnesses to publish their messages before next round
round_witness_time = 1

# number of clients that need to be active for an epoch to continue on.
# if the number of clients goes below this number, we initiate a Cooldown and then back to WaitingForMembers.
# this should be adjusted alongside max_round_train_time, because one client will train a lot slower
# than 100.
min_clients = 1

# minumum number of clients required before we transition from WaitingForMembers to Warmup.
# must be equal to or greater than min_clients
init_min_clients = 1

# what percent of nodes are dedicated to verifying correctness. always set to 0 for now.
verification_percent = 0

# how many nodes are selected each round to publish witness proofs
witness_nodes = 1

# the total number of training data batches per-step. this also determines your maximum number of clients.
# the batch size will linearly increase from global_batch_size_start to global_batch_size_end over
# global_batch_size_warmup_tokens tokens
global_batch_size_start = 8
global_batch_size_end = 8
global_batch_size_warmup_tokens = 0

# the total number of training steps to partake in. this is used for the LR schedule in the model section too.
total_steps = 25000
```

## Model

```toml
# so far only LLMs are supported.
[model.LLM]
architecture = "HfLlama"
data_type = "Pretraining"
max_seq_len = 2048

[model.LLM.checkpoint.Hub]
repo_id = "emozilla/llama2-20m-init"

[model.LLM.data_location.Http]
token_size_in_bytes = "TwoBytes"
shuffle = "DontShuffle"

[model.LLM.data_location.Http.location.Gcp]
bucket_name = "nous-pretraining-public-us"
filter_directory = "fineweb-edu-tokenized-llama2"

[model.LLM.lr_schedule.Cosine]
base_lr = 4.0e-4
warmup_steps = 250
warmup_init_lr = 0.0
total_steps = 25000
final_lr = 4.0e-5

# only the DisTrO optimizer is supported when training models on Psyche.
[model.LLM.optimizer.Distro]
clip_grad_norm = 1.0
compression_decay = 0.999
compression_chunk = 64
compression_topk = 8
quantize_1bit = true
```
