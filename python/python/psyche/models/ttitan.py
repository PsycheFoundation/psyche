import torch
import json
import os

import torch.distributed.checkpoint as dcp

from .causal_lm import CausalLM, PretrainedSourceRepoFiles, PretrainedSourceStateDict
from typing import Union, Iterable, Optional
from torch.distributed.device_mesh import DeviceMesh
from torch.distributed.tensor import DTensor, Replicate, distribute_tensor

from .hf_transformers import auto_config_from_dict

import torchtitan
from torchtitan.config import JobConfig
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
    def __init__(self, model, config, config_tt, job_config, device, amp):
        self.model = model
        self.config = config
        self.config_tt = config_tt
        self.job_config = job_config
        self.device = device
        self.amp = amp

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

        config_tt = None
        seq_len = override_max_position_embeddings
        if (
            config.model_type == "llama"
            or config.model_type == "qwen2"
            or config.model_type == "seed_oss"
        ):
            if seq_len is not None:
                seq_len = config.max_position_embeddings
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
                # need these from our fork to specify arbitrary shapes once we figure out packaging
                # head_dim=config.head_dim,
                # hidden_dim=config.intermediate_size,
                # use_qkv_bias=config.model_type == "qwen2"
                # or (config.model_type == "seed_oss" and config.attention_bias),
                max_seq_len=seq_len,
            )

        if config_tt is None or config.model_type not in TRAIN_SPEC_FN:
            raise ValueError(f"Unsupported model_type `{config.model_type}`")
        train_spec = TRAIN_SPEC_FN[config.model_type]()

        model = None
        with torch.device("meta"), set_default_dtype(torch.float32):
            model = train_spec.model_cls(config_tt)

        model_param_count, _ = config_tt.get_nparams_and_flops(
            model, config_tt.max_seq_len
        )

        job_config = JobConfig()
        job_config.training.seq_len = seq_len
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

        return TorchtitanAuto(model, config, config_tt, job_config, device, amp)
