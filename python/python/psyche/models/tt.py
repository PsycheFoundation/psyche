import torch
import os
from dataclasses import asdict
import torch.distributed.checkpoint as dcp
import json

# Import from psyche
from . import CausalLM, PretrainedSourceRepoFiles, PretrainedSourceStateDict

# Import from torchtitan
from torchtitan.config_manager import JobConfig
from torchtitan.distributed import ParallelDims
from torchtitan.protocols import train_spec as train_spec_module
from torchtitan.components.checkpoint import ModelWrapper
from torchtitan.models.llama3.model.args import TransformerModelArgs as LlamaTransformerModelArgs

# Typing
from typing import Union, Iterable, Optional, Tuple


class Torchtitan(CausalLM):
    """
    A wrapper for Torchtitan models to be used within the psyche training framework.
    """

    def __init__(self, model: torch.nn.Module, model_args: object, loss_fn: callable, world_mesh: object):
        self.model = model
        self.model_args = model_args
        self.loss_fn = loss_fn
        self.world_mesh = world_mesh

    @staticmethod
    def from_pretrained(
        source: Union[PretrainedSourceRepoFiles, PretrainedSourceStateDict],
        device: torch.device,
        dp: int = 1,
        tp: int = 1,
        override_max_position_embeddings: Optional[int] = None,
        param_dtype: torch.dtype = torch.bfloat16,
        reduce_dtype: torch.dtype = torch.float32,
        fsdp_modules: Optional[Iterable[str]] = None,
        **kwargs,
    ):
        if isinstance(source, PretrainedSourceStateDict):
            raise NotImplementedError("Torchtitan loading from in-memory state_dict is not supported.")


        config_json = None
        checkpoint_path = None
        source: Iterable[str] = source.files
        for file in source:
            basename = os.path.basename(file).lower()
            if basename.endswith(".distcp"):
                checkpoint_path = os.path.dirname(file)
            elif basename == "config.json":
                config_json = open(file, "r", encoding="utf-8").read()

        if config_json is None:
            raise RuntimeError("No config.json present")
        config: dict = json.loads(config_json)

        if checkpoint_path is None:
            raise RuntimeError("No .distcp files found")

        # 1. Create a minimal JobConfig to drive model creation and parallelization.

        job_config = JobConfig()

        if config["model_type"] == "llama":
            intermediate_size = config["intermediate_size"]
            if intermediate_size == 14336:
                ffn_dim_multiplier = 1.3
                multiple_of = 1024
            elif intermediate_size == 28672:
                ffn_dim_multiplier = 1.3
                multiple_of = 4096
            elif intermediate_size == 53248:
                ffn_dim_multiplier = 1.2
                multiple_of = 4096
            else:
                raise ValueError("Unknown mapping of `intermediate_size`")
            
            model_args = LlamaTransformerModelArgs(
                dim=config["hidden_size"],
                n_layers=config["num_hidden_layers"],
                n_heads=config["num_attention_heads"],
                n_kv_heads=config["num_key_value_heads"],
                ffn_dim_multiplier=ffn_dim_multiplier,
                multiple_of=multiple_of,
                rope_theta=config["rope_theta"],
                rope_scaling="rope_scaling" in config and config["rope_scaling"] is not None,
                max_seq_len=config["max_position_embeddings"],
                eos_id=config["eos_token_id"],
                norm_eps=config["rms_norm_eps"]
            )

        job_config.training.mixed_precision_param = "bfloat16" if param_dtype == torch.bfloat16 else "float32"
        job_config.parallelism.data_parallel_shard_degree = -1
        job_config.parallelism.data_parallel_replicate_degree = dp
        job_config.parallelism.tensor_parallel_degree = tp
        
        # We need a tokenizer to correctly initialize model_args (e.g., vocab_size).
        # This assumes the default tokenizer path in JobConfig is valid.
        # if not os.path.exists(job_config.model.tokenizer_path):
        #     raise FileNotFoundError(f"Tokenizer not found at {job_config.model.tokenizer_path}. "
        #                             "Torchtitan requires a tokenizer to initialize the model.")

        # 2. Setup distributed environment and device mesh using Torchtitan's utilities.
        world_size = int(os.environ.get("WORLD_SIZE", 1))
        parallel_dims = ParallelDims(dp_shard=-1, dp_replicate=dp, tp=tp, world_size=world_size)
        world_mesh = parallel_dims.build_mesh(device_type=device.type)

        # 3. Get model spec, build tokenizer, and configure model arguments.
        train_spec = train_spec_module.get_train_spec(job_config.model.name)
        model_cls = train_spec.cls


        # 4. Instantiate the model on the meta device.
        with torch.device("meta"):
            model = model_cls(model_args)

        # 5. Apply parallelism. Torchtitan's function handles FSDP/TP setup and materialization.
        model = train_spec.parallelize_fn(model, world_mesh, parallel_dims, job_config)
        model.to_empty(device=device)

        # 6. Load weights from the checkpoint directory.
        # This uses torch.distributed.checkpoint and expects a standard Torchtitan
        # checkpoint format that includes a "model" key.
        states_to_load = {"model": ModelWrapper([model])}
        print(f"Loading Torchtitan model weights from: {checkpoint_path}")
        dcp.load(states_to_load, checkpoint_id=checkpoint_path)

        # 7. Create the appropriate loss function for the model.
        loss_fn = train_spec.build_loss_fn(job_config)

        return Torchtitan(model, model_args, loss_fn, world_mesh)

    def forward(
        self,
        input_ids: torch.Tensor,
        labels: Optional[torch.Tensor],
        num_logits_to_keep: Optional[int] = 0,
        loss_scale: Optional[float] = None,
    ) -> Tuple[torch.Tensor, Optional[torch.Tensor]]:
        """
        Performs a forward pass, returning logits and optionally calculating loss.
        """
        logits = self.model(tokens=input_ids)
        loss = None
        if labels is not None:
            loss = self.loss_fn(logits, labels)
            if loss is not None and loss_scale is not None and loss_scale != 1.0:
                loss /= loss_scale

        if num_logits_to_keep and num_logits_to_keep > 0:
            logits = logits[..., :num_logits_to_keep]

        return logits, loss

    def named_parameters(self) -> dict[str, torch.Tensor]:
        return dict(self.model.named_parameters())

    def get_config(self) -> dict:
        return asdict(self.model_args)

    def train(self):
        """Sets the model to training mode."""
        self.model.train()