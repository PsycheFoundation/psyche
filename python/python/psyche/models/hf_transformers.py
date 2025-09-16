import torch
import json
import os

from .causal_lm import CausalLM, PretrainedSourceRepoFiles, PretrainedSourceStateDict
from transformers import AutoModelForCausalLM, PreTrainedModel
from typing import Union, Iterable, Optional, Tuple
from safetensors import safe_open
from safetensors.torch import load_file as safe_load_file
from transformers.models.auto.configuration_auto import CONFIG_MAPPING
from torch.distributed import init_device_mesh
from torch.distributed.device_mesh import DeviceMesh
from torch.distributed._composable.fsdp import fully_shard, MixedPrecisionPolicy
from torch.distributed.tensor import DTensor, distribute_tensor
from torch.distributed.algorithms._checkpoint.checkpoint_wrapper import (
    apply_activation_checkpointing,
    _CHECKPOINT_PREFIX,
)


# adapted from https://github.com/pytorch/torchtitan/blob/49c6d6fc15ef644e5c3b1003ad4e0d9ea5fcb9a9/torchtitan/parallelisms/parallel_dims.py#L48
def build_mesh(device_type, pp=1, dp_replicate=1, dp_shard=1, cp=1, tp=1) -> DeviceMesh:
    dims = []
    names = []
    for d, name in zip(
        [pp, dp_replicate, dp_shard, cp, tp],
        ["pp", "dp_replicate", "dp_shard", "cp", "tp"],
    ):
        if d > 1:
            dims.append(d)
            names.append(name)

    names = tuple(names)
    mesh = init_device_mesh(device_type, dims, mesh_dim_names=names)

    # Create all the submesh here to ensure all required process groups are
    # initialized:
    # Mesh for data loading (no communication on this mesh)
    dp_mesh_dim_names = []
    # Mesh for param sharding
    dp_shard_cp_mesh_dim_names = []
    # Mesh for loss all-reduce
    dp_cp_mesh_dim_names = []

    if dp_replicate > 1:
        dp_mesh_dim_names.append("dp_replicate")
        dp_cp_mesh_dim_names.append("dp_replicate")
    if dp_shard > 1:
        dp_mesh_dim_names.append("dp_shard")
        dp_shard_cp_mesh_dim_names.append("dp_shard")
        dp_cp_mesh_dim_names.append("dp_shard")
    if cp > 1:
        dp_shard_cp_mesh_dim_names.append("cp")
        dp_cp_mesh_dim_names.append("cp")

    if dp_mesh_dim_names != []:
        mesh[tuple(dp_mesh_dim_names)]._flatten(mesh_dim_name="dp")
    if dp_shard_cp_mesh_dim_names != []:
        mesh[tuple(dp_shard_cp_mesh_dim_names)]._flatten(mesh_dim_name="dp_shard_cp")
    if dp_cp_mesh_dim_names != []:
        mesh[tuple(dp_cp_mesh_dim_names)]._flatten(mesh_dim_name="dp_cp")

    return mesh


class HfTransformersAuto(CausalLM):

    def __init__(self, model, config, world_mesh: DeviceMesh):
        self.model = model
        self.config = config
        self.world_mesh = world_mesh

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
            state_dict = source.state_dict
            config_json = source.config_json
        else:
            state_dict = {}
            config_json = None
            source: Iterable[str] = source.files
            for file in source:
                basename = os.path.basename(file).lower()
                if basename.endswith(".safetensors"):
                    with safe_open(file, framework="pt") as f:
                        metadata = f.metadata()
                    if metadata is not None and metadata.get("format") != "pt":
                        raise RuntimeError("Not a PyTorch safetensors file")
                    state_dict.update(safe_load_file(file))
                elif basename == "config.json":
                    config_json = open(file, "r", encoding="utf-8").read()

        if config_json is None:
            raise RuntimeError("No config.json present")
        config: dict = json.loads(config_json)

        model_type = config.get("model_type")
        if model_type is None:
            raise RuntimeError("model_type not present in config.json")
        try:
            config_class = CONFIG_MAPPING[model_type]
        except KeyError:
            raise ValueError(f"Unknown model_type {model_type}")

        config = config_class.from_dict(config)

        if override_max_position_embeddings:
            config.max_position_embeddings = override_max_position_embeddings

        with torch.device("meta"):
            model: torch.nn.Module = AutoModelForCausalLM.from_config(
                config, attn_implementation=attn_implementation
            )
        if device.type == "cuda":
            torch.cuda.set_device(device)

        world_mesh = None
        if tp != 1 or dp != 1:
            # world_mesh = build_mesh("cuda", dp_replicate=dp, tp=tp)
            world_mesh = build_mesh("cuda", dp_shard=dp)
            if tp != 1:
                raise RuntimeError("TP not supported in HfTransformers yet")
                # model.tensor_parallel(world_mesh[("tp",)])

            if dp != 1:
                mp_policy = MixedPrecisionPolicy(
                    param_dtype=param_dtype, reduce_dtype=reduce_dtype
                )
                fsdp_config = {
                    # "mesh": world_mesh[tuple(("dp_replicate",))],
                    "mesh": world_mesh[tuple(("dp_shard",))],
                    "mp_policy": mp_policy,
                }

                if fsdp_modules is None:
                    if isinstance(model, PreTrainedModel):
                        fsdp_modules = model._no_split_modules
                    if hasattr(model, "model"):
                        if isinstance(model.model, PreTrainedModel):
                            fsdp_modules = model.model._no_split_modules
                if fsdp_modules is None:
                    raise RuntimeError("Could not determine models to apply FSDP to")

                # seems to break with latest transformers, let's fall back to their activation checkpointing
                # apply_activation_checkpointing(
                #     model,
                #     check_fn=lambda module: module.__class__.__name__ in fsdp_modules,
                # )

                for module in model.modules():
                    if module.__class__.__name__ in fsdp_modules:
                        fully_shard(module, **fsdp_config)
                model = fully_shard(model, **fsdp_config)
            else:
                model = model.to(dtype=param_dtype)
        else:
            # if not sharding, apply param_dtype
            model = model.to(dtype=param_dtype)

        # move the (potentially sharded) meta model to the device
        model.to_empty(device=device)

        # HACK: apply RoPE parameters after meta device transition.
        # because transformers does this in __init__() (which is ignored on meta)
        # rather than post_init() or init_weights(), there (doesn't appear) to
        # be a general way to initialize static calculated buffers.
        # might be a problem for arbitrary models.
        # this is highly britle, someone plz fix

        def reinit_rope(module):
            if (
                hasattr(module, "inv_freq")
                and hasattr(module, "config")
                and hasattr(module, "attention_scaling")
                and hasattr(module, "rope_init_fn")
            ):
                inv_freq, attention_scaling = module.rope_init_fn(
                    module.config, device, **getattr(module, "rope_kwargs", {})
                )
                module.inv_freq.copy_(inv_freq)
                module.attention_scaling = attention_scaling

                # llama scaling needs this
                if hasattr(module, "original_inv_freq"):
                    module.original_inv_freq = module.inv_freq

        for module in model.modules():
            reinit_rope(module)
        reinit_rope(model)

        if model.supports_gradient_checkpointing:
            model._set_gradient_checkpointing(True)

        # for super large models, loading the entire model in RAM nproc times can CPU OOM
        # TODO: switch to use torch.distributed.checkpoint.state_dict_loader.load()

        for name, dest in model.state_dict().items():
            source: Optional[torch.Tensor] = state_dict.get(name)
            if source is None:
                raise RuntimeError(f"Missing parameter {name}")

            if isinstance(dest, DTensor):
                source = distribute_tensor(
                    source, device_mesh=dest.device_mesh, placements=dest.placements
                )

            dest.copy_(source)

        return HfTransformersAuto(model, config, world_mesh)

    def forward(
        self,
        input_ids: torch.Tensor,
        labels: Optional[torch.Tensor],
        position_ids: Optional[torch.Tensor] = None,
        sequence_lengths: Optional[list[list[int]]] = None,
        num_logits_to_keep: Optional[int] = None,
        loss_scale: Optional[float] = None,
    ) -> Tuple[torch.Tensor, Optional[torch.Tensor]]:
        if self.world_mesh:
            if self.world_mesh.mesh_dim_names:
                if "dp_shard" in self.world_mesh.mesh_dim_names:
                    dp_shard = self.world_mesh[tuple(("dp_shard",))]
                    size = dp_shard.size()
                    rank = dp_shard.get_local_rank()

                    # do FSDP data sharding
                    shard_size = input_ids.shape[0] // size
                    start_row = rank * shard_size
                    input_ids = input_ids.narrow(0, start_row, shard_size).contiguous()
                    if labels is not None:
                        labels = labels.narrow(0, start_row, shard_size).contiguous()
                    if position_ids is not None:
                        position_ids = position_ids.narrow(
                            0, start_row, shard_size
                        ).contiguous()

        num_logits_to_keep = 0 if num_logits_to_keep is None else num_logits_to_keep
        ret = self.model(
            input_ids,
            labels=labels,
            position_ids=position_ids,
            logits_to_keep=num_logits_to_keep,  # name changed in 4.50
            return_dict=True,
        )
        if ret.loss and loss_scale:
            ret.loss /= loss_scale
        return (ret.logits, ret.loss)

    def named_parameters(self) -> dict[str, torch.Tensor]:
        params = dict(self.model.named_parameters())
        # undo activation checkpoint wrapping
        return {k.replace(_CHECKPOINT_PREFIX, ""): v for k, v in params.items()}

    def train(self):
        self.model.train()

    def get_config(self):
        return self.config.to_dict()
