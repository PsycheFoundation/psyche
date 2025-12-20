import torch
import json
import os
from contextlib import contextmanager

import torch.distributed.checkpoint as dcp

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
from torchtitan.models.llama3 import get_train_spec as get_llama3_train_spec
from torchtitan.models.llama3.model.args import TransformerModelArgs, RoPEScalingArgs
from torchtitan.models.qwen3 import get_train_spec as get_qwen3_train_spec
from torchtitan.models.qwen3.model.args import Qwen3ModelArgs
from torchtitan.tools.utils import get_device_info, set_default_dtype

TRAIN_SPEC_FN = {
    "llama": get_llama3_train_spec,
    "qwen2": get_llama3_train_spec,
    "seed_oss": get_llama3_train_spec,
}


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
        if (
            config.model_type == "llama"
            or config.model_type == "qwen2"
            or config.model_type == "seed_oss"
        ):
            if seq_len is None:
                raise ValueError(
                    "Could not determine an appropriate max sequence length for Torchtitan model"
                )
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
                        scaling_factor=config.rope_scaling.factor,
                        low_freq_factor=config.rope_scaling.low_freq_factor,
                        high_freq_factor=config.rope_scaling.high_freq_factor,
                        original_max_position_embeddings=config.rope_scaling.original_max_position_embeddings,
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
        if config_tt is None:
            raise ValueError(f"Unsupported model_type `{config.model_type}`")
        return config_tt

    def convert(
        self, state_dict: Optional[dict[str, torch.Tensor]]
    ) -> dict[str, torch.Tensor]:
        state_dict = self.model.state_dict() if state_dict is None else state_dict
        train_spec = TRAIN_SPEC_FN[self.config.model_type]()
        sd_adapter = train_spec.state_dict_adapter(self.config_tt, hf_assets_path=None)
        return sd_adapter.to_hf(state_dict)

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
        if device.type == "cuda":
            torch.cuda.set_device(device)

        config_json = None
        if isinstance(source, PretrainedSourceStateDict):
            raise RuntimeError("Unimplemented")
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

        if config.model_type not in TRAIN_SPEC_FN:
            raise ValueError(f"Unsupported model_type `{config.model_type}`")
        train_spec = TRAIN_SPEC_FN[config.model_type]()

        model = None
        with torch.device("meta"), set_default_dtype(torch.float32):
            model = train_spec.model_cls(config_tt)

        model_param_count, _ = config_tt.get_nparams_and_flops(
            model, config_tt.max_seq_len
        )

        job_config = JobConfig()
        job_config.training.seq_len = config_tt.max_seq_len
        job_config.compile.enamble = True
        job_config.compile.components = ["loss", "model"]
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

            state_dict = model.state_dict()
            hf_state_dict = sd_adapter.to_hf(state_dict)

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
            model.load_state_dict(state_dict)

        loss_fn = build_cross_entropy_loss(job_config)

        return TorchtitanAuto(
            model, loss_fn, config, config_tt, job_config, device, amp, parallel_dims
        )

    def named_parameters(self) -> dict[str, torch.Tensor]:
        params = dict(self.model.named_parameters())
        # undo activation checkpoint wrapping
        return {k.replace(_CHECKPOINT_PREFIX, ""): v for k, v in params.items()}

    def train(self):
        self.model.train()

    def get_config(self):
        return self.config.to_dict()

    def _get_rope_cache_handles(self):
        if not hasattr(self, "model"):
            return None, None, None
        for cache_attr, precompute_attr in (
            ("freqs_cis", "_precompute_freqs_cis"),
            ("rope_cache", "_precompute_rope_cache"),
        ):
            if hasattr(self.model, cache_attr):
                cache_tensor = getattr(self.model, cache_attr)
                precompute_fn = getattr(self.model, precompute_attr, None)
                return cache_attr, cache_tensor, precompute_fn
        return None, None, None

    def _maybe_extend_rope_cache(self, target_seq_len: int) -> None:
        if target_seq_len <= 0:
            return

        cache_attr, cache_tensor, precompute_fn = self._get_rope_cache_handles()
        if cache_attr is None or cache_tensor is None or precompute_fn is None:
            return

        current_len = cache_tensor.shape[0]
        if target_seq_len <= current_len:
            return

        if hasattr(self.model, "model_args"):
            self.model.model_args.max_seq_len = target_seq_len

        cache_device = cache_tensor.device
        with torch.device(cache_device):
            new_cache = precompute_fn()
        setattr(self.model, cache_attr, new_cache)

    @contextmanager
    def _temporarily_truncate_rope_cache(self, target_seq_len: int):
        cache_attr, cache_tensor, _ = self._get_rope_cache_handles()
        if cache_attr is None or cache_tensor is None:
            yield
            return

        original_cache = cache_tensor
        truncated_cache = cache_tensor[:target_seq_len]
        setattr(self.model, cache_attr, truncated_cache)
        try:
            yield
        finally:
            setattr(self.model, cache_attr, original_cache)

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
            target_seq_len = input_ids.shape[-1]
            if position_ids is not None:
                target_seq_len = max(
                    target_seq_len, int(torch.max(position_ids).item()) + 1
                )
            self._maybe_extend_rope_cache(target_seq_len)
            with self._temporarily_truncate_rope_cache(target_seq_len):
                with self.amp:
                    pred = self.model(
                        tokens=input_ids.contiguous(),
                        position_ids=(
                            position_ids.contiguous()
                            if position_ids is not None
                            else None
                        ),
                    )
                    if num_logits_to_keep:
                        pred = pred[:, -num_logits_to_keep, :]
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
