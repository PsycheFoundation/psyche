import torch
import json
import os
from contextlib import contextmanager, nullcontext

import torch.distributed.checkpoint as dcp
import torch.nn.functional as F

from .causal_lm import CausalLM, PretrainedSourceRepoFiles, PretrainedSourceStateDict
from typing import Tuple, Union, Iterable, Optional
from torch.distributed.device_mesh import DeviceMesh
from torch.distributed.tensor import DTensor, Replicate, distribute_tensor
from torch.distributed.algorithms._checkpoint.checkpoint_wrapper import (
    _CHECKPOINT_PREFIX,
)

from .hf_transformers import auto_config_from_dict

import torchtitan
from torchtitan.config import JobConfig
from torchtitan.components.loss import build_cross_entropy_loss
from torchtitan.distributed import ParallelDims
from torchtitan.distributed.utils import maybe_enable_amp
from torchtitan.experiments.gpt_oss import get_train_spec as get_gpt_oss_train_spec
from torchtitan.experiments.gpt_oss.model.args import GptOssModelArgs
from torchtitan.experiments.qwen3_next import (
    get_train_spec as get_qwen3_next_train_spec,
)
from torchtitan.experiments.qwen3_next.model.args import Qwen3NextModelArgs
from torchtitan.models.deepseek_v3 import get_train_spec as get_deepseek_v3_train_spec
from torchtitan.models.deepseek_v3.model.args import DeepSeekV3ModelArgs
from torchtitan.models.llama3 import get_train_spec as get_llama3_train_spec
from torchtitan.models.llama3.model.args import TransformerModelArgs, RoPEScalingArgs
from torchtitan.models.moe import MoEArgs
from torchtitan.models.qwen3 import get_train_spec as get_qwen3_train_spec
from torchtitan.models.qwen3.model.args import Qwen3ModelArgs
from torchtitan.tools.utils import get_device_info, set_default_dtype

TRAIN_SPEC_FN = {
    "llama": get_llama3_train_spec,
    "qwen2": get_llama3_train_spec,
    "seed_oss": get_llama3_train_spec,
    "qwen3_moe": get_qwen3_train_spec,
    "deepseek_v3": get_deepseek_v3_train_spec,
    "gpt_oss": get_gpt_oss_train_spec,
    "qwen3_next": get_qwen3_next_train_spec,
}

COMPILE_PREFIX = "_orig_mod."


class TorchtitanAuto(CausalLM):
    def __init__(
        self, model, loss_fn, config, config_tt, job_config, device, amp, parallel_dims
    ):
        self.model = model
        self.loss_fn = loss_fn
        self.config = config
        self.config_tt = config_tt
        self.job_config = job_config
        self.device = device
        self.amp = amp
        self.parallel_dims = parallel_dims

    @staticmethod
    def convert_config(config, override_max_position_embeddings: Optional[int] = None):
        config_tt = None
        seq_len = (
            override_max_position_embeddings
            or getattr(config, "max_position_embeddings", None)
            or getattr(config, "max_sequence_length", None)
        )
        if seq_len is None:
            raise ValueError(
                "Could not determine an appropriate max sequence length for Torchtitan model"
            )
        if (
            config.model_type == "llama"
            or config.model_type == "qwen2"
            or config.model_type == "seed_oss"
        ):
            config_tt = TransformerModelArgs(
                dim=config.hidden_size,
                n_layers=config.num_hidden_layers,
                n_heads=config.num_attention_heads,
                n_kv_heads=config.num_key_value_heads,
                vocab_size=config.vocab_size,
                norm_eps=config.rms_norm_eps,
                rope_theta=config.rope_theta,
                rope_scaling_args=(
                    RoPEScalingArgs(
                        scaling_factor=config.rope_scaling["factor"],
                        low_freq_factor=config.rope_scaling["low_freq_factor"],
                        high_freq_factor=config.rope_scaling["high_freq_factor"],
                        original_max_position_embeddings=config.rope_scaling[
                            "original_max_position_embeddings"
                        ],
                    )
                    if config.rope_scaling is not None
                    else RoPEScalingArgs()
                ),
                head_dim=config.head_dim,
                hidden_dim=config.intermediate_size,
                use_qkv_bias=config.model_type == "qwen2"
                or (config.model_type == "seed_oss" and config.attention_bias),
                max_seq_len=seq_len,
            )
        elif config.model_type == "qwen3_moe":
            config_tt = Qwen3ModelArgs(
                dim=config.hidden_size,
                n_layers=config.num_hidden_layers,
                n_heads=config.num_attention_heads,
                n_kv_heads=config.num_key_value_heads,
                vocab_size=config.vocab_size,
                head_dim=config.head_dim,
                hidden_dim=config.intermediate_size,
                norm_eps=config.rms_norm_eps,
                rope_theta=config.rope_theta,
                max_seq_len=seq_len,
                moe_enabled=True,
                moe_inter_dim=config.moe_intermediate_size,
                moe_args=MoEArgs(
                    num_experts=config.num_experts,
                    num_shared_experts=0,  # qwen3 has no shared experts
                    top_k=config.num_experts_per_tok,
                    score_func="softmax",
                    route_norm=True,
                    score_before_experts=False,
                    load_balance_coeff=config.router_aux_loss_coef,
                ),
            )
        elif config.model_type == "deepseek_v3":
            config_tt = DeepSeekV3ModelArgs(
                max_seq_len=seq_len,
                vocab_size=config.vocab_size,
                dim=config.hidden_size,
                inter_dim=config.intermediate_size,
                n_layers=config.num_hidden_layers,
                n_dense_layers=config.first_k_dense_replace,
                n_heads=config.num_attention_heads,
                norm_eps=config.rms_norm_eps,
                n_expert_groups=config.n_group,
                n_limited_groups=config.topk_group,
                q_lora_rank=config.q_lora_rank,
                kv_lora_rank=config.kv_lora_rank,
                qk_nope_head_dim=config.qk_nope_head_dim,
                qk_rope_head_dim=config.qk_rope_head_dim,
                v_head_dim=config.v_head_dim,
                original_seq_len=config.rope_scaling[
                    "original_max_position_embeddings"
                ],
                rope_theta=config.rope_theta,
                rope_factor=config.rope_scaling["factor"],
                beta_fast=config.rope_scaling["beta_fast"],
                beta_slow=config.rope_scaling["beta_slow"],
                mscale=config.rope_scaling["mscale"],
                moe_args=MoEArgs(
                    num_experts=config.n_routed_experts,
                    num_shared_experts=config.n_shared_experts,
                    top_k=config.num_experts_per_tok,
                    score_func=config.scoring_func,
                    route_scale=config.routed_scaling_factor,
                    score_before_experts=False,
                ),
            )
        elif config.model_type == "gpt_oss":
            config_tt = GptOssModelArgs(
                max_seq_len=seq_len,
                vocab_size=config.vocab_size,
                dim=config.hidden_size,
                moe_inter_dim=config.intermediate_size,
                n_layers=config.num_hidden_layers,
                norm_eps=config.rms_norm_eps,
                swiglu_limit=config.swiglu_limit,
                head_dim=config.head_dim,
                n_heads=config.num_attention_heads,
                n_kv_heads=config.num_key_value_heads,
                sliding_window_size=config.sliding_window,
                original_seq_len=config.rope_scaling[
                    "original_max_position_embeddings"
                ],
                rope_theta=config.rope_theta,
                rope_factor=config.rope_scaling["factor"],
                beta_fast=config.rope_scaling["beta_fast"],
                beta_slow=config.rope_scaling["beta_slow"],
                moe_args=MoEArgs(
                    num_experts=config.num_local_experts,
                    num_shared_experts=0,
                    score_func="softmax",
                    score_before_experts=False,
                    top_k=config.num_experts_per_tok,
                    load_balance_coeff=config.router_aux_loss_coef,
                ),
            )
        elif config.model_type == "qwen3_next":
            config_tt = Qwen3NextModelArgs(
                dim=config.hidden_size,
                n_layers=config.num_hidden_layers,
                n_heads=config.num_attention_heads,
                n_kv_heads=config.num_key_value_heads,
                vocab_size=config.vocab_size,
                head_dim=config.head_dim,
                hidden_dim=config.intermediate_size,
                hidden_act=config.hidden_act,
                norm_eps=config.rms_norm_eps,
                rope_theta=config.rope_theta,
                partial_rotary_factor=config.partial_rotary_factor,
                max_seq_len=seq_len,
                moe_enabled=True,
                moe_inter_dim=config.moe_intermediate_size,
                decoder_sparse_step=config.decoder_sparse_step,
                full_attention_interval=config.full_attention_interval,
                linear_num_key_heads=config.linear_num_key_heads,
                linear_num_value_heads=config.linear_num_value_heads,
                linear_key_head_dim=config.linear_key_head_dim,
                linear_value_head_dim=config.linear_value_head_dim,
                linear_conv_kernel_dim=config.linear_conv_kernel_dim,
                moe_args=MoEArgs(
                    num_experts=config.num_experts,
                    num_shared_experts=1,  # constant?
                    top_k=config.num_experts_per_tok,
                    score_func="softmax",
                    route_norm=True,
                    score_before_experts=False,
                    shared_gate=True,
                    load_balance_coeff=config.router_aux_loss_coef,
                ),
            )
        if config_tt is None:
            raise ValueError(f"Unsupported model_type `{config.model_type}`")
        return config_tt

    def convert(
        self, state_dict: Optional[dict[str, torch.Tensor]]
    ) -> dict[str, torch.Tensor]:
        state_dict = self.model.state_dict() if state_dict is None else state_dict
        # Strip wrapper prefixes before converting to HF format
        state_dict = {
            k.replace(_CHECKPOINT_PREFIX, "").replace(COMPILE_PREFIX, ""): v
            for k, v in state_dict.items()
        }
        train_spec = TRAIN_SPEC_FN[self.config.model_type]()
        sd_adapter = train_spec.state_dict_adapter(self.config_tt, hf_assets_path=None)
        state_dict = sd_adapter.to_hf(state_dict)
        return {k: v.to(torch.bfloat16) for k, v in state_dict.items()}

    @staticmethod
    def _load_into_model(model, state_dict):
        # map from clean keys to the actual model keys
        # and load into model

        model_sd = model.state_dict()
        clean_to_actual = {
            k.replace(_CHECKPOINT_PREFIX, "").replace(COMPILE_PREFIX, ""): k
            for k in model_sd.keys()
        }

        sorted_keys = sorted(state_dict.keys())
        for idx, k in enumerate(sorted_keys):
            source = state_dict[k]
            actual_key = clean_to_actual.get(k)
            if actual_key is not None:
                dest = model_sd[actual_key]

                if isinstance(dest, DTensor):
                    source = distribute_tensor(
                        source, device_mesh=dest.device_mesh, placements=dest.placements
                    )

                dest.copy_(source)
            else:
                raise RuntimeError(f"Missing parameter {actual_key}")

    @staticmethod
    def from_pretrained(
        source: Union[PretrainedSourceRepoFiles, PretrainedSourceStateDict],
        device: torch.device,
        attn_implementation: str,
        dp: int = 1,
        tp: int = 1,
        override_max_position_embeddings: Optional[int] = None,
        param_dtype: torch.dtype = torch.bfloat16,
        reduce_dtype: torch.dtype = torch.float32,
        fsdp_modules: Optional[Iterable[str]] = None,
    ):
        config_json = None
        if isinstance(source, PretrainedSourceStateDict):
            state_dict = source.state_dict
            config_json = source.config_json
        else:
            for file in source.files:
                basename = os.path.basename(file).lower()
                if basename == "config.json":
                    config_json = open(file, "r", encoding="utf-8").read()

        if config_json is None:
            raise RuntimeError("No config.json present")
        config = auto_config_from_dict(json.loads(config_json))
        config_tt = TorchtitanAuto.convert_config(
            config, override_max_position_embeddings
        )

        job_config = JobConfig()
        job_config.training.seq_len = config_tt.max_seq_len
        job_config.compile.enable = True
        job_config.compile.components = ["model", "loss"]
        job_config.compile.fullgraph = False
        job_config.activation_checkpoint.mode = "full"
        job_config.parallelism.data_parallel_shard_degree = dp
        job_config.parallelism.tensor_parallel_degree = tp

        parallel_dims = ParallelDims(
            dp_replicate=1,
            dp_shard=dp,
            cp=1,
            tp=tp,
            pp=1,
            ep=1,
            etp=1,
            world_size=dp * tp,  # fake, but only used for validation
        )

        config_tt.update_from_config(job_config)

        if config.model_type not in TRAIN_SPEC_FN:
            raise ValueError(f"Unsupported model_type `{config.model_type}`")
        train_spec = TRAIN_SPEC_FN[config.model_type]()

        model = None
        with torch.device("meta"), set_default_dtype(torch.float32):
            model = train_spec.model_cls(config_tt)
        torch.cuda.set_device(device)

        model_param_count, _ = config_tt.get_nparams_and_flops(
            model, config_tt.max_seq_len
        )

        if dp != 1 or tp != 1:
            model = train_spec.parallelize_fn(model, parallel_dims, job_config)

        model.to_empty(device=device)
        with torch.no_grad():
            model.init_weights(buffer_device=None)
        model.train()

        print(
            f"created `{config.model_type}`, size: {model_param_count:,} total parameters"
        )

        device_type, _ = get_device_info()
        amp = maybe_enable_amp(
            parallel_dims=parallel_dims,
            mixed_precision_param=(
                "bfloat16" if param_dtype == torch.bfloat16 else torch.float32
            ),
            device_type=device_type,
        )

        if isinstance(source, PretrainedSourceRepoFiles):
            sd_adapter = train_spec.state_dict_adapter(config_tt, hf_assets_path=None)

            model_sd_clean = {
                k.replace(_CHECKPOINT_PREFIX, "").replace(COMPILE_PREFIX, ""): v
                for k, v in model.state_dict().items()
            }
            hf_state_dict = sd_adapter.to_hf(model_sd_clean)

            path = None
            for x in source.files:
                if os.path.basename(x).lower().endswith(".safetensors"):
                    path = os.path.dirname(x)
            if path is None:
                raise RuntimeError(
                    f"Could not determine .safetensors root directory for `{source.files}`"
                )

            hf_storage_reader = dcp.HuggingFaceStorageReader(path)
            dcp.load(hf_state_dict, storage_reader=hf_storage_reader)

            state_dict = sd_adapter.from_hf(hf_state_dict)
            TorchtitanAuto._load_into_model(model, state_dict)
        else:
            # state_dict already in TT format
            TorchtitanAuto._load_into_model(model, state_dict)

        loss_fn = build_cross_entropy_loss(job_config)

        return TorchtitanAuto(
            model, loss_fn, config, config_tt, job_config, device, amp, parallel_dims
        )

    def named_parameters(self) -> dict[str, torch.Tensor]:
        params = dict(self.model.named_parameters())
        # undo activation checkpoint and torch.compile wrapping
        return {
            k.replace(_CHECKPOINT_PREFIX, "").replace(COMPILE_PREFIX, ""): v
            for k, v in params.items()
        }

    def train(self):
        self.model.train()

    def get_config(self):
        return self.config.to_dict()

    def forward(
        self,
        input_ids: torch.Tensor,
        labels: Optional[torch.Tensor],
        position_ids: Optional[torch.Tensor] = None,
        sequence_lengths: Optional[list[list[int]]] = None,
        num_logits_to_keep: Optional[int] = None,
        loss_scale: Optional[float] = None,
    ) -> Tuple[Optional[torch.Tensor], Optional[torch.Tensor]]:
        if self.parallel_dims.world_mesh:
            if self.parallel_dims.world_mesh.mesh_dim_names:
                if "dp_shard" in self.parallel_dims.world_mesh.mesh_dim_names:
                    dp_shard = self.parallel_dims.world_mesh[tuple(("dp_shard",))]
                    size = dp_shard.size()
                    rank = dp_shard.get_local_rank()

                    # do FSDP data sharding
                    shard_size = input_ids.shape[0] // size
                    start_row = rank * shard_size
                    input_ids = input_ids.narrow(0, start_row, shard_size)
                    if labels is not None:
                        labels = labels.narrow(0, start_row, shard_size)
                    if position_ids is not None:
                        position_ids = position_ids.narrow(0, start_row, shard_size)
        try:
            with self.amp, torch.cuda.device(input_ids.device.index):
                pred = self.model(
                    tokens=input_ids.contiguous(),
                    position_ids=(
                        position_ids.contiguous() if position_ids is not None else None
                    ),
                )
                if num_logits_to_keep:
                    pred = pred[:, -num_logits_to_keep:, :]
                loss = None
                if labels is not None:
                    if labels.shape != pred.shape[:2]:
                        raise ValueError(
                            f"Labels shape {labels.shape} does not match logits shape {pred.shape[:2]}"
                        )
                    if pred.shape[1] < 2:
                        raise ValueError(
                            "Sequence length must be >= 2 for causal shift"
                        )
                    shift_logits = pred[:, :-1, :].contiguous()
                    shift_labels = labels[:, 1:].contiguous()
                    loss = self.loss_fn(shift_logits, shift_labels)
        except Exception as e:
            import traceback

            print(f"[{self.device}]: {e}")
            traceback.print_exception(e)
            raise e
        if loss_scale:
            loss = loss / loss_scale
        return (pred, loss)
