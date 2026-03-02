## make-tiny-init reference

The `make-tiny-init.py` script creates tiny versions of any psyche/hf/torchtitan-compatible model for fast iteration. It produces a tiny init checkpoint, a `config.json`, and a ready-to-use [run config](../enduser/run-config.md).

### Model Input

The script accepts one of the following model formats:

```bash
# HF repo slug: easiest way, downloads config + tokenizer
nix develop .#python --command python scripts/make-tiny-init.py --repo Qwen/Qwen3-30B-A3B --save /tmp/test-qwen

# from a raw HF config.json, use --tokenizer to include a tokenizer
nix develop .#python --command python scripts/make-tiny-init.py  --config /path/to/config.json --save /tmp/tiny --tokenizer Qwen/Qwen3-30B-A3B

# from a torchtitan job toml
nix develop .#python --command python scripts/make-tiny-init.py  --toml /path/to/train.toml --save /tmp/tiny-run

# from a psyche run config (reads model.LLM.checkpoint.Hub.repo_id)
nix develop .#python --command python scripts/make-tiny-init.py  --psyche /path/to/state.toml --save /tmp/tiny-llama
```

Read the comments inside the make-tiny-init script for more information.

> **Tip:** use `--preserve vocab_size` if you want to train on real tokenized data — otherwise the tiny vocab won't match your tokenizer.
