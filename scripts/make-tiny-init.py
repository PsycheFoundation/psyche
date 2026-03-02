"""
Create a minimized version of any model for debugging and testing.

This script reads a model config from various sources (HF config.json, torchtitan TOML,
HF repo slug, or psyche training TOML), shrinks all dimensions to tiny values,
creates a tiny init checkpoint, and generates a ready-to-use psyche training TOML.

It ensures that this model is still parallelizable with TP=8.

Unknown fields in models are not touched, so this should work with any model.

Examples:
    # from HF repo — produces checkpoint/ + train.toml
    python scripts/make-tiny-init.py --repo Qwen/Qwen3-30B-A3B --save /tmp/test-qwen

    # from config.json with explicit tokenizer
    python scripts/make-tiny-init.py --config /path/to/config.json --save /tmp/tiny --tokenizer Qwen/Qwen3-30B-A3B

    # from torchtitan TOML (also produces tiny-train.toml)
    python scripts/make-tiny-init.py --toml /path/to/train.toml --save /tmp/tiny-run

    # from psyche training TOML
    python scripts/make-tiny-init.py --psyche /path/to/state.toml --save /tmp/tiny-llama

    # shrink config without making checkpoint or training TOML
    python scripts/make-tiny-init.py --config /path/to/config.json --save /tmp/tiny --no-checkpoint --no-toml
"""

import argparse
import json
import os
import shutil

import tomllib

# rules for shrinking HF config items. we map each value to either a fixed int or a function that modifies it.
# any field not listed isn't touched
#
# important constraints:
# 1. head counts must always be divisible by 8, to keep TP=8 working
# 2. hidden_size = num_attention_heads * head_dim
# 3. num_attention_heads % num_key_value_heads == 0
# 4. num_experts_per_tok <= num_experts

NUM_HEADS = 8
HEAD_DIM = 16
FIELD_RULES = {
    # heads - constraint 1
    "num_attention_heads": NUM_HEADS,
    "num_key_value_heads": NUM_HEADS,
    "linear_num_key_heads": NUM_HEADS,
    "linear_num_value_heads": NUM_HEADS,
    # hidden dims - constraint 2
    "head_dim": HEAD_DIM,
    "hidden_size": NUM_HEADS * HEAD_DIM,
    # ffn
    "intermediate_size": 256,
    "moe_intermediate_size": 256,
    # minimal layer counts
    "num_hidden_layers": 2,
    "first_k_dense_replace": lambda orig: min(orig, 1),
    # tiny sequence len
    "max_position_embeddings": 256,
    # tiny vocab
    "vocab_size": 256,
    # for MoEs
    "num_experts": 8,
    "n_routed_experts": 8,
    "num_local_experts": 8,
    "num_experts_per_tok": lambda orig: min(orig, 2),  # constraint #4
    "n_shared_experts": lambda orig: min(orig, 1),
    "n_group": 1,
    "topk_group": 1,
    # LoRA / projection rank for models like deepseek & qwen3_next
    "q_lora_rank": 16,
    "kv_lora_rank": 16,
    "qk_nope_head_dim": 16,
    "qk_rope_head_dim": 16,
    "v_head_dim": 16,
    "linear_key_head_dim": 16,
    "linear_value_head_dim": 16,
}


def make_config_tiny(config_dict: dict, preserve: set[str] | None = None) -> dict:
    """Shrink an HF config dict to minimal dimensions that still make the model trainable."""
    preserve = preserve or set()
    result = dict(config_dict)

    for field, rule in FIELD_RULES.items():
        if field not in result or field in preserve:
            continue
        if callable(rule):
            result[field] = rule(result[field])
        else:
            result[field] = rule

    # shrink rope_scaling.original_max_position_embeddings to match
    if "rope_scaling" in result and isinstance(result["rope_scaling"], dict):
        rs = dict(result["rope_scaling"])
        if (
            "original_max_position_embeddings" in rs
            and "max_position_embeddings" not in preserve
        ):
            rs["original_max_position_embeddings"] = result.get(
                "max_position_embeddings", 256
            )
        result["rope_scaling"] = rs

    ## make sure --preserve flags don't break anything
    # constraint #2
    if "hidden_size" in result and "num_attention_heads" in result:
        assert result["hidden_size"] % result["num_attention_heads"] == 0, (
            f"hidden_size ({result['hidden_size']}) must be divisible by "
            f"num_attention_heads ({result['num_attention_heads']}). "
            f"did you --preserve a value here that would break this constraint?"
        )
    # constraint #3
    if "num_attention_heads" in result and "num_key_value_heads" in result:
        assert result["num_attention_heads"] % result["num_key_value_heads"] == 0, (
            f"num_attention_heads ({result['num_attention_heads']}) must be divisible by "
            f"num_key_value_heads ({result['num_key_value_heads']}). "
            f"did you --preserve a value here that would break this constraint?"
        )

    return result


def create_checkpoint(
    config_dict: dict, save_path: str, dtype_str: str, device: str | None
):
    """create a torchtitan init checkpoint from a config dict."""
    import torch
    import torch.distributed.checkpoint as dcp
    from torch.distributed.checkpoint import HuggingFaceStorageWriter
    from torchtitan.config import JobConfig
    from psyche.models.hf_transformers import auto_config_from_dict
    from psyche.models.ttitan import TorchtitanAuto, TRAIN_SPEC_FN

    dtype_map = {
        "bfloat16": torch.bfloat16,
        "float32": torch.float32,
        "float16": torch.float16,
    }
    save_dtype = dtype_map.get(dtype_str, torch.bfloat16)

    config = auto_config_from_dict(config_dict)
    config_tt = TorchtitanAuto.convert_config(config)

    job_config = JobConfig()
    job_config.training.seq_len = config_tt.max_seq_len
    config_tt.update_from_config(job_config)

    if config.model_type not in TRAIN_SPEC_FN:
        raise ValueError(f"unknown model_type `{config.model_type}`")
    train_spec = TRAIN_SPEC_FN[config.model_type]()

    torch.set_default_dtype(torch.float32)
    if device:
        torch.set_default_device(device)

    model = train_spec.model_cls(config_tt)
    with torch.no_grad():
        model.init_weights(buffer_device=None)

    model_param_count, _ = config_tt.get_nparams_and_flops(model, config_tt.max_seq_len)
    print(
        f"created really tiny small tiny `{config.model_type}`, size: {model_param_count:,} total parameters. awwww it's so cute! :3"
    )

    sd_adapter = train_spec.state_dict_adapter(config_tt, hf_assets_path=None)
    state_dict = model.state_dict()
    del model

    hf_state_dict = {
        k: v.to(save_dtype) for k, v in sd_adapter.to_hf(state_dict).items()
    }
    storage_writer = HuggingFaceStorageWriter(path=save_path)
    dcp.save(hf_state_dict, storage_writer=storage_writer, checkpoint_id=save_path)

    print(f"saved init checkpoint to {save_path}")


def find_hf_config_json(toml_config: dict) -> str | None:
    """get the path to a config.json from a torchtitan TOML job config.

    checks model.hf_assets_path, then checkpoint.initial_load_path.
    """
    model_section = toml_config.get("model", {})
    checkpoint_section = toml_config.get("checkpoint", {})

    for base_path in [
        model_section.get("hf_assets_path"),
        checkpoint_section.get("initial_load_path"),
    ]:
        if base_path is None:
            continue
        candidate = os.path.join(base_path, "config.json")
        if os.path.isfile(candidate):
            return candidate

    return None


# this fn doesn't use toml parsing so that we preserve all comments etc that the TOML might have
def rewrite_toml(
    original_toml_path: str, toml_config: dict, tiny_assets_path: str, output_path: str
):
    """rewrite the torchtitan TOML to point at the new tiny checkpoint."""
    with open(original_toml_path) as f:
        content = f.read()

    model = toml_config.get("model", {})
    checkpoint = toml_config.get("checkpoint", {})

    old_hf_assets = model.get("hf_assets_path")
    if old_hf_assets is not None:
        content = content.replace(
            f'hf_assets_path = "{old_hf_assets}"',
            f'hf_assets_path = "{tiny_assets_path}"',
        )

    old_initial_load = checkpoint.get("initial_load_path")
    if old_initial_load is not None:
        content = content.replace(
            f'initial_load_path = "{old_initial_load}"',
            f'initial_load_path = "{tiny_assets_path}"',
        )

    old_flavor = model.get("flavor")
    if old_flavor is not None:
        content = content.replace(
            f'flavor = "{old_flavor}"',
            f'flavor = "tiny"  # was: "{old_flavor}"',
        )

    with open(output_path, "w") as f:
        f.write(content)


def print_changes(original: dict, shrunk: dict, preserve: set[str]):
    changed = []
    for key in sorted(FIELD_RULES.keys()):
        if key in original and key not in preserve:
            old, new = original[key], shrunk.get(key)
            if old != new:
                changed.append(f"  {key}: {old} -> {new}")
    if changed:
        model_type = original.get("model_type", "unknown")
        print(f"shrinking `{model_type}` config:")
        print("\n".join(changed))
    else:
        print(
            "no fields changed. either this model is already tiny, or all shrinkable fields were preserved"
        )


def detect_architecture(config_dict: dict) -> str:
    """auto-detect whether to use Torchtitan or HfAuto based on model_type."""
    model_type = config_dict.get("model_type")
    if model_type is None:
        return "HfAuto"
    try:
        from psyche.models.ttitan import TRAIN_SPEC_FN

        if model_type in TRAIN_SPEC_FN:
            return "Torchtitan"
    except ImportError:
        pass
    return "HfAuto"


def generate_psyche_toml(
    config_dict: dict,
    checkpoint_path: str,
    architecture: str,
    total_steps: int,
) -> str:
    """generate a psyche training TOML config string."""
    max_seq_len = config_dict.get("max_position_embeddings", 256)

    return f"""\
[config]
warmup_time = 5
cooldown_time = 5
epoch_time = 60
max_round_train_time = 15
round_witness_time = 1
min_clients = 1
init_min_clients = 1
verification_percent = 0
witness_nodes = 0
global_batch_size_start = 4
global_batch_size_end = 4
global_batch_size_warmup_tokens = 0
total_steps = {total_steps}
waiting_for_members_extra_time = 3

[model.LLM]
architecture = "{architecture}"
data_type = "Pretraining"
max_seq_len = {max_seq_len}
cold_start_warmup_steps = 0

data_location = "Dummy"

[model.LLM.checkpoint.Hub]
repo_id = "{checkpoint_path}"

[model.LLM.lr_schedule.Cosine]
base_lr = 4.0e-4
warmup_steps = 10
warmup_init_lr = 0.0
total_steps = {total_steps}
final_lr = 4.0e-5

[model.LLM.optimizer.AdamW]
betas = [0.9, 0.95]
weight_decay = 0.1
eps = 1e-8
clip_grad_norm = 1.0
"""


def looks_like_hf_slug(s: str) -> bool:
    """check if a string looks like an HF repo slug (owner/name) rather than a local path."""
    parts = s.split("/")
    # HF slugs are exactly owner/name with no path separators beyond that
    return len(parts) == 2 and not s.startswith((".", "/"))


def load_config_json(source: str) -> dict:
    """load a config.json from a local path or HF repo slug."""
    local = os.path.join(source, "config.json") if os.path.isdir(source) else source
    if os.path.isfile(local):
        with open(local) as f:
            return json.load(f)

    if looks_like_hf_slug(source):
        from huggingface_hub import hf_hub_download

        path = hf_hub_download(source, "config.json")
        with open(path) as f:
            return json.load(f)

    raise FileNotFoundError(f"can't find config.json at {source!r}")


def resolve_input(args, parser) -> tuple[dict, str | None, dict | None]:
    """Resolve the input source into (config_dict, tokenizer_source, torchtitan_toml).

    tokenizer_source is either a local path, an HF slug, or None.
    torchtitan_toml is only set when --toml was used (needed for rewrite_toml later).
    """
    if args.repo:
        return load_config_json(args.repo), args.repo, None

    if args.config:
        with open(args.config) as f:
            config_dict = json.load(f)
        return config_dict, os.path.dirname(os.path.abspath(args.config)), None

    if args.toml:
        with open(args.toml, "rb") as f:
            toml_config = tomllib.load(f)

        config_json_path = find_hf_config_json(toml_config)
        if config_json_path is None:
            hf_path = toml_config.get("model", {}).get("hf_assets_path", "<not set>")
            init_path = toml_config.get("checkpoint", {}).get(
                "initial_load_path", "<not set>"
            )
            parser.error(
                f"couldn't find config.json. checked both of \n"
                f"  model.hf_assets_path = {hf_path}\n"
                f"  checkpoint.initial_load_path = {init_path}\n"
                f"one of these must contain a config.json file"
            )

        print(f"found config at {config_json_path}")
        with open(config_json_path) as f:
            config_dict = json.load(f)
        return config_dict, os.path.dirname(config_json_path), toml_config

    if args.psyche:
        with open(args.psyche, "rb") as f:
            psyche_config = tomllib.load(f)

        repo_id = (
            psyche_config.get("model", {})
            .get("LLM", {})
            .get("checkpoint", {})
            .get("Hub", {})
            .get("repo_id")
        ) or (
            psyche_config.get("model", {})
            .get("LLM", {})
            .get("checkpoint", {})
            .get("P2P", {})
            .get("repo_id")
        )
        if repo_id is None:
            parser.error(
                "couldn't find model.LLM.checkpoint.(Hub | P2P).repo_id in psyche TOML"
            )

        return load_config_json(repo_id), repo_id, None

    # unreachable — argparse enforces the mutually exclusive group
    assert False


def copy_local_tokenizer(src_dir: str, dst_dir: str) -> bool:
    """copy tokenizer files from src_dir to dst_dir. returns True if any were found."""
    if not os.path.isfile(os.path.join(src_dir, "tokenizer.json")):
        return False

    for tok_file in os.listdir(src_dir):
        if "token" in tok_file.lower() or tok_file in (
            "special_tokens_map.json",
            "tokenizer_config.json",
        ):
            src = os.path.join(src_dir, tok_file)
            dst = os.path.join(dst_dir, tok_file)
            if os.path.isfile(src):
                shutil.copy2(src, dst)
    print(f"copied tokenizer from {src_dir}")
    return True


def save_tokenizer(
    tokenizer_source: str | None, checkpoint_dir: str, explicit_tokenizer: str | None
):
    """resolve and save tokenizer files into checkpoint_dir.

    Priority: explicit --tokenizer flag > tokenizer_source from input resolution.
    """
    source = explicit_tokenizer or tokenizer_source
    if source is None:
        print("no tokenizer source found, skipping tokenizer")
        return

    # try local copy first
    if os.path.isdir(source) and copy_local_tokenizer(source, checkpoint_dir):
        return

    # try as HF slug or pretrained identifier
    try:
        from transformers import AutoTokenizer

        AutoTokenizer.from_pretrained(source).save_pretrained(checkpoint_dir)
        print(f"saved tokenizer from {source}")
    except Exception as e:
        print(f"warning: couldn't load tokenizer from {source}: {e}")


def main():
    parser = argparse.ArgumentParser(
        description="create a tiny model from an HF repo, config.json, torchtitan TOML, or psyche TOML"
    )

    input_group = parser.add_mutually_exclusive_group(required=True)
    input_group.add_argument(
        "--repo", type=str, help="HF repo slug (e.g. Qwen/Qwen3-30B-A3B)"
    )
    input_group.add_argument("--config", type=str, help="path to a raw HF config.json")
    input_group.add_argument(
        "--toml", type=str, help="path to a torchtitan job config TOML"
    )
    input_group.add_argument(
        "--psyche", type=str, help="path to a psyche training config TOML"
    )

    parser.add_argument(
        "--save",
        type=str,
        required=True,
        help="output directory for all generated files",
    )
    parser.add_argument(
        "--preserve",
        type=str,
        nargs="*",
        default=[],
        help="config fields to keep at their original values. e.g --preserve vocab_size",
    )
    parser.add_argument(
        "--tokenizer",
        type=str,
        help="tokenizer to include. HF repo slug or local path",
    )
    parser.add_argument(
        "--dtype",
        type=str,
        default="bfloat16",
        help="dtype for saved weights (default: bfloat16)",
    )
    parser.add_argument(
        "--device",
        type=str,
        default=None,
        help="device to init model on",
    )
    parser.add_argument(
        "--no-checkpoint",
        action="store_true",
        help="skip creating init weights",
    )
    parser.add_argument(
        "--no-toml",
        action="store_true",
        help="skip generating the psyche training TOML",
    )
    parser.add_argument(
        "--architecture",
        type=str,
        help="override architecture in generated TOML (default: auto-detect)",
    )
    parser.add_argument(
        "--total-steps",
        type=int,
        default=100,
        help="total_steps for generated TOML (default: 100)",
    )
    parser.add_argument(
        "--push",
        type=str,
        help="HF repo to push the tiny checkpoint to",
    )
    parser.add_argument(
        "--private",
        action="store_true",
        help="push tiny checkpoint as a private HF repo",
    )

    args = parser.parse_args()
    preserve = set(args.preserve)
    os.makedirs(args.save, exist_ok=True)

    # phase 1: resolve input
    config_dict, tokenizer_source, torchtitan_toml = resolve_input(args, parser)

    # phase 2: shrink
    tiny_config = make_config_tiny(config_dict, preserve=preserve)
    print_changes(config_dict, tiny_config, preserve)

    # phase 3: write config.json + optionally create checkpoint
    checkpoint_dir = os.path.join(args.save, "checkpoint")
    os.makedirs(checkpoint_dir, exist_ok=True)

    config_out = os.path.join(checkpoint_dir, "config.json")
    with open(config_out, "w") as f:
        json.dump(tiny_config, f, indent=2)
    print(f"wrote tiny config to {config_out}")

    if not args.no_checkpoint:
        create_checkpoint(tiny_config, checkpoint_dir, args.dtype, args.device)
        save_tokenizer(tokenizer_source, checkpoint_dir, args.tokenizer)

    # phase 4: generate psyche training TOML
    if not args.no_toml:
        architecture = args.architecture or detect_architecture(tiny_config)
        toml_out = os.path.join(args.save, "train.toml")
        toml_content = generate_psyche_toml(
            tiny_config,
            checkpoint_path="./checkpoint/",
            architecture=architecture,
            total_steps=args.total_steps,
        )
        with open(toml_out, "w") as f:
            f.write(toml_content)
        print(f"wrote psyche training TOML to {toml_out}")

    # phase 5: rewrite torchtitan TOML (only for --toml input)
    if torchtitan_toml is not None:
        ckpt_dir_abs = os.path.abspath(checkpoint_dir)
        tt_toml_out = os.path.join(args.save, "tiny-train.toml")
        rewrite_toml(args.toml, torchtitan_toml, ckpt_dir_abs, tt_toml_out)
        print(f"wrote torchtitan TOML to {tt_toml_out}")

    # phase 6: push to HF
    if args.push:
        from huggingface_hub import HfApi

        api = HfApi()
        api.create_repo(
            repo_id=args.push,
            private=args.private,
            repo_type="model",
            exist_ok=True,
        )
        api.upload_folder(
            folder_path=checkpoint_dir, repo_id=args.push, repo_type="model"
        )
        print(f"pushed to https://huggingface.co/{args.push}")


if __name__ == "__main__":
    main()
