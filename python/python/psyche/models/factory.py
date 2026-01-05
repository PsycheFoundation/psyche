import torch

from .causal_lm import PretrainedSourceRepoFiles, PretrainedSourceStateDict, CausalLM
from typing import Optional, Iterable


def make_causal_lm(
    architecture: str,
    source: PretrainedSourceRepoFiles | PretrainedSourceStateDict,
    device: torch.device | str | int,
    attn_implementation: str,
    dp: int = 1,
    tp: int = 1,
    override_max_position_embeddings: Optional[int] = None,
    param_dtype: torch.dtype = torch.bfloat16,
    reduce_dtype: torch.dtype = torch.float32,
    fsdp_modules: Optional[Iterable[str]] = None,
) -> CausalLM:
    if not isinstance(device, torch.device):
        device = torch.device(device if isinstance(device, str) else f"cuda:{device}")
    if architecture == "HfAuto":
        from .hf_transformers import HfTransformersAuto

        return HfTransformersAuto.from_pretrained(
            source=source,
            device=device,
            attn_implementation=attn_implementation,
            dp=dp,
            tp=tp,
            override_max_position_embeddings=override_max_position_embeddings,
            param_dtype=param_dtype,
            reduce_dtype=reduce_dtype,
            fsdp_modules=fsdp_modules,
        )
    elif architecture == "Torchtitan":
        from .ttitan import TorchtitanAuto

        return TorchtitanAuto.from_pretrained(
            source=source,
            device=device,
            attn_implementation=attn_implementation,
            dp=dp,
            tp=tp,
            override_max_position_embeddings=override_max_position_embeddings,
            param_dtype=param_dtype,
            reduce_dtype=reduce_dtype,
            fsdp_modules=fsdp_modules,
        )
    raise ValueError(f"Unknown architecture {architecture}")
