from transformers import (
    LlamaConfig,
    LlamaForCausalLM,
    AutoTokenizer,
    DeepseekV3Config,
    DeepseekV3ForCausalLM,
)
from transformers.models.llama.modeling_llama import LlamaDecoderLayer
from torch import nn
import argparse
import torch
import math
import json


def _init_normal(module, std: float, cutoff_factor: float = 3.0):
    with torch.no_grad():
        cutoff = std * cutoff_factor
        weight = module.weight
        weight.normal_(0, std)
        torch.clamp_(weight, min=-cutoff, max=cutoff)
        if hasattr(module, "bias") and module.bias is not None:
            module.bias.zero_()


def initialize_llama_weights(model: LlamaForCausalLM, config: LlamaConfig):
    """Initialize model weights using the "Mitchell" initialization scheme"""

    wte_std = 1 / math.sqrt(config.hidden_size)
    _init_normal(model.model.embed_tokens, std=wte_std)

    for layer_id, layer in enumerate(model.model.layers):
        layer: LlamaDecoderLayer = layer

        attn_std = 1 / math.sqrt(config.hidden_size)
        _init_normal(layer.self_attn.q_proj, std=attn_std)
        _init_normal(layer.self_attn.k_proj, std=attn_std)
        _init_normal(layer.self_attn.v_proj, std=attn_std)

        attn_out_std = 1 / (math.sqrt(2 * config.hidden_size * (layer_id + 1)))
        _init_normal(layer.self_attn.o_proj, std=attn_out_std)

        ff_std = 1 / math.sqrt(config.hidden_size)
        _init_normal(layer.mlp.gate_proj, std=ff_std)
        _init_normal(layer.mlp.up_proj, std=ff_std)

        ff_out_std = 1 / (
            math.sqrt(2 * layer.mlp.down_proj.in_features * (layer_id + 1))
        )
        _init_normal(layer.mlp.down_proj, std=ff_out_std)

        nn.init.ones_(layer.input_layernorm.weight)
        nn.init.ones_(layer.post_attention_layernorm.weight)

    nn.init.ones_(model.model.norm.weight)

    if model.lm_head is not None:
        lm_std = 1 / math.sqrt(config.hidden_size)
        _init_normal(model.lm_head, std=lm_std)


def initialize_deepseek_weights(model: DeepseekV3ForCausalLM, config: DeepseekV3Config):
    """Initialize model weights using the "Mitchell" initialization scheme"""

    wte_std = 1 / math.sqrt(config.hidden_size)
    _init_normal(model.model.embed_tokens, std=wte_std)

    for layer_id, layer in enumerate(model.model.layers):

        if config.q_lora_rank is None:
            attn_std = 1 / math.sqrt(config.hidden_size)
            _init_normal(layer.self_attn.q_proj, std=attn_std)
        else:
            attn_qa_lora_std = 1 / math.sqrt(config.hidden_size)
            attn_qb_lora_std = 1 / math.sqrt(config.q_lora_rank)
            nn.init.ones_(layer.self_attn.q_a_layernorm.weight)
            _init_normal(layer.self_attn.q_a_proj, std=attn_qa_lora_std)
            _init_normal(layer.self_attn.q_b_proj, std=attn_qb_lora_std)

        attn_kva_lora_std = 1 / math.sqrt(config.hidden_size)
        attn_kvb_lora_std = 1 / math.sqrt(config.kv_lora_rank)
        nn.init.ones_(layer.self_attn.kv_a_layernorm.weight)
        _init_normal(layer.self_attn.kv_a_proj_with_mqa, std=attn_kva_lora_std)
        _init_normal(layer.self_attn.kv_b_proj, std=attn_kvb_lora_std)

        attn_out_std = 1 / (math.sqrt(2 * config.hidden_size * (layer_id + 1)))
        _init_normal(layer.self_attn.o_proj, std=attn_out_std)

        ff_std = 1 / math.sqrt(config.hidden_size)
        _init_normal(layer.mlp.gate_proj, std=ff_std)
        _init_normal(layer.mlp.up_proj, std=ff_std)

        ff_out_std = 1 / (
            math.sqrt(2 * layer.mlp.down_proj.in_features * (layer_id + 1))
        )
        _init_normal(layer.mlp.down_proj, std=ff_out_std)

        nn.init.ones_(layer.input_layernorm.weight)
        nn.init.ones_(layer.post_attention_layernorm.weight)

    nn.init.ones_(model.model.norm.weight)

    if model.lm_head is not None:
        lm_std = 1 / math.sqrt(config.hidden_size)
        _init_normal(model.lm_head, std=lm_std)


def main(args):
    if not args.config:
        raise RuntimeError("No config provided")
    config = json.load(open(args.config))
    model_type = config["model_type"]

    if model_type == "llama":
        config = LlamaConfig.from_pretrained(args.config)
    elif model_type == "deepseek_v3":
        config = DeepseekV3Config.from_pretrained(args.config)
    else:
        raise ValueError(f"Unsupported model type `{model_type}`")

    torch.set_default_dtype(args.dtype)
    if args.device:
        torch.set_default_device(args.device)

    print("Initializing random model...")
    if model_type == "llama":
        model = LlamaForCausalLM(config)
    elif model_type == "deepseek_v3":
        model = DeepseekV3ForCausalLM(config)

    if model_type == "llama":
        print("OLMo initialization...")
        initialize_llama_weights(model, config)
    elif model_type == "deepseek_v3":
        print("Dense MLA Mitchell initialization...")
        initialize_deepseek_weights(model, config)

    print(model)
    total_params = sum(p.numel() for p in model.parameters())

    print(f"Model has {total_params} parameters")
    if args.repo:
        model.push_to_hub(args.repo, private=args.private)
        if args.tokenizer:
            AutoTokenizer.from_pretrained(args.tokenizer).push_to_hub(
                args.repo, private=args.private
            )
    if args.save:
        model.save_pretrained(args.save)
        if args.tokenizer:
            AutoTokenizer.from_pretrained(args.tokenizer).save_pretrained(args.save)


args = argparse.ArgumentParser()
args.add_argument(
    "--config",
    type=str,
    help="source config repo or path to JSON config",
)
args.add_argument("--repo", type=str, help="destination repo")
args.add_argument("--save", type=str, help="save to local")
args.add_argument("--private", action="store_true", help="push as a private repo")
args.add_argument("--dtype", type=int, default=torch.bfloat16, help="torch dtype")
args.add_argument("--device", type=str, help="device to init on")
args.add_argument("--tokenizer", type=str, help="tokenizer")

main(args.parse_args())
