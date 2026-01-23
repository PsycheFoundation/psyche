import torch
import json
import os

from .causal_lm import CausalLM, PretrainedSourceRepoFiles, PretrainedSourceStateDict
from transformers import (
    AutoModelForCausalLM,
    GradientCheckpointingLayer,
    PreTrainedModel,
)
from typing import Union, Iterable, Optional, Tuple
from safetensors import safe_open
from safetensors.torch import load_file as safe_load_file
from transformers.models.auto.configuration_auto import CONFIG_MAPPING
from torch.distributed import init_device_mesh
from torch.distributed.fsdp.wrap import ModuleWrapPolicy
from torch.distributed.device_mesh import DeviceMesh
from torch.distributed._composable.fsdp import fully_shard, MixedPrecisionPolicy
from torch.distributed.tensor import DTensor, Replicate, distribute_tensor
from torch.distributed.tensor.parallel import (
    parallelize_module,
    ColwiseParallel,
    RowwiseParallel,
)
from torch.distributed.algorithms._checkpoint.checkpoint_wrapper import (
    apply_activation_checkpointing,
    _CHECKPOINT_PREFIX,
)
from liger_kernel.transformers.monkey_patch import (
    _apply_liger_kernel_to_instance,
    MODEL_TYPE_TO_APPLY_LIGER_FN,
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


def auto_config_from_dict(config: dict):
    model_type = config.get("model_type")
    if model_type is None:
        raise RuntimeError("model_type not present in config.json")
    try:
        config_class = CONFIG_MAPPING[model_type]
    except KeyError:
        raise ValueError(f"Unknown model_type {model_type}")

    return config_class.from_dict(config)


class HfTransformersAuto(CausalLM):
    def __init__(self, model, config, world_mesh: DeviceMesh, device: torch.device):
        self.model = model
        self.config = config
        self.world_mesh = world_mesh
        self.device = device

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
        config = auto_config_from_dict(json.loads(config_json))
        if override_max_position_embeddings:
            config.max_position_embeddings = override_max_position_embeddings

        with torch.device("meta"):
            model: torch.nn.Module = AutoModelForCausalLM.from_config(
                config,
                attn_implementation=attn_implementation,
            )
        torch.cuda.set_device(device)

        world_mesh = None
        if tp != 1 or dp != 1:
            world_mesh = build_mesh("cuda", dp_shard=dp, tp=tp)

            tp_mesh = world_mesh["tp"] if tp > 1 else None
            dp_shard_mesh = world_mesh["dp_shard"] if dp > 1 else None

            if tp != 1:
                tp_mesh = world_mesh["tp"]

                if config.model_type != "llama" and config.model_type != "seed_oss":
                    raise ValueError(
                        f"Tensor parallelism not supported for model type `{config.model_type}` (yet)"
                    )
                if config.num_attention_heads % tp != 0:
                    raise ValueError(
                        f"TP degree {tp} must divide num_attention_heads {config.num_attention_heads}"
                    )
                if config.num_key_value_heads % tp != 0:
                    raise ValueError(
                        f"TP degree {tp} must divide num_key_value_heads {config.num_key_value_heads}"
                    )

                layer_plan = {
                    "self_attn.q_proj": ColwiseParallel(),
                    "self_attn.k_proj": ColwiseParallel(),
                    "self_attn.v_proj": ColwiseParallel(),
                    "self_attn.o_proj": RowwiseParallel(),
                    "mlp.gate_proj": ColwiseParallel(),
                    "mlp.up_proj": ColwiseParallel(),
                    "mlp.down_proj": RowwiseParallel(),
                }

                for layer in model.model.layers:
                    parallelize_module(layer, tp_mesh, parallelize_plan=layer_plan)

                parallelize_module(
                    model,
                    tp_mesh,
                    parallelize_plan={
                        "lm_head": ColwiseParallel(output_layouts=Replicate()),
                    },
                )

            if dp != 1:
                mp_policy = MixedPrecisionPolicy(
                    param_dtype=param_dtype, reduce_dtype=reduce_dtype
                )
                fsdp_config = {
                    "mesh": dp_shard_mesh,
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

                for module in model.modules():
                    if module.__class__.__name__ in fsdp_modules:
                        fully_shard(module, **fsdp_config)
                model = fully_shard(model, **fsdp_config)
            else:
                # pure TP
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
            model.gradient_checkpointing_enable()

        if config.model_type in MODEL_TYPE_TO_APPLY_LIGER_FN:
            print(f"Applying liger kernels to model type `{config.model_type}`")
            no_tp = tp == 1
            _apply_liger_kernel_to_instance(
                model=model,
                fused_linear_cross_entropy=no_tp,  # liger fused ce can't deal with mixed tensor/dtensors which happens in non-pure-fsdp mode
            )

        # compile the loss, greatly reduces mem usage for large vocabularies
        model.loss_function = torch.compile(model.loss_function)

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

        return HfTransformersAuto(model, config, world_mesh, device)

    def forward(
        self,
        input_ids: torch.Tensor,
        labels: Optional[torch.Tensor],
        position_ids: Optional[torch.Tensor] = None,
        sequence_lengths: Optional[list[list[int]]] = None,
        num_logits_to_keep: Optional[int] = None,
        loss_scale: Optional[float] = None,
    ) -> Tuple[Optional[torch.Tensor], Optional[torch.Tensor]]:
        if self.world_mesh:
            if self.world_mesh.mesh_dim_names:
                if "dp_shard" in self.world_mesh.mesh_dim_names:
                    dp_shard = self.world_mesh[tuple(("dp_shard",))]
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

        num_logits_to_keep = 0 if num_logits_to_keep is None else num_logits_to_keep

        # need to wrap in a device context or get triton errors when using liger
        # see https://github.com/linkedin/Liger-Kernel/issues/593#issuecomment-2770160474
        with torch.cuda.device(input_ids.device.index):
            try:
                ret = self.model(
                    input_ids.contiguous(),
                    labels=labels.contiguous() if labels is not None else None,
                    position_ids=(
                        position_ids.contiguous() if position_ids is not None else None
                    ),
                    logits_to_keep=num_logits_to_keep,  # name changed in 4.50
                    return_dict=True,
                    use_cache=False,
                )
            except Exception as e:
                import traceback

                print(f"[{self.device}]: {e}")
                traceback.print_exception(e)
                raise e
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

    def convert(
        self, state_dict: Optional[dict[str, torch.Tensor]]
    ) -> dict[str, torch.Tensor]:
        return state_dict if state_dict is not None else self.model.state_dict()
