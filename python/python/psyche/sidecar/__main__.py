import argparse
from typing import Optional
import torch
import json
import os
import torch.distributed as dist

from datetime import timedelta
from .. import (
    make_causal_lm,
    PretrainedSourceRepoFiles,
    PretrainedSourceStateDict,
    Trainer,
    DistroResult,
    start_process_watcher,
)
from .api import (
    DistroResultsMetadata,
    ForwardOperation,
    Hyperparameters,
    OptimizeOperation,
    TrainOperation,
)

# These values should be in sync with include/c10/core/ScalarType.h
# https://github.com/pytorch/pytorch/blob/a8d6afb511a69687bbb2b7e88a3cf67917e1697e/c10/core/ScalarType.h#L57
DTYPE_MAPPING = {
    0: torch.uint8,
    3: torch.int,
    4: torch.int64,
    5: torch.half,
    6: torch.float,
    7: torch.double,
    11: torch.bool,
    15: torch.bfloat16,
}


def receive_distro_results(
    results_len: int,
    metadata: DistroResultsMetadata,
    device: torch.device,
) -> list[list[DistroResult]]:
    assert len(metadata.sparse_idx_size) == len(metadata.sparse_val_size)
    assert len(metadata.sparse_val_size) == len(metadata.xshape)
    assert len(metadata.xshape) == len(metadata.totalk)
    sparse_idxs = []
    sparse_vals = []
    params_len = len(metadata.sparse_idx_size)

    for param_index in range(params_len):
        sparse_idx_size = (results_len,) + tuple(metadata.sparse_idx_size[param_index])
        sparse_val_size = (results_len,) + tuple(metadata.sparse_val_size[param_index])

        sparse_idx = torch.empty(
            sparse_idx_size,
            dtype=DTYPE_MAPPING[metadata.sparse_idx_dtype],
            device=device,
        )
        sparse_val = torch.empty(
            sparse_val_size,
            dtype=DTYPE_MAPPING[metadata.sparse_val_dtype],
            device=device,
        )
        dist.broadcast(sparse_idx, 0)
        dist.broadcast(sparse_val, 0)

        sparse_idxs.append(sparse_idx.chunk(results_len, dim=0))
        sparse_vals.append(sparse_val.chunk(results_len, dim=0))

    results = []
    for result_index in range(results_len):
        result = []
        for param_index in range(params_len):
            xshape = metadata.xshape[param_index]
            totalk = metadata.totalk[param_index]
            result.append(
                DistroResult(
                    sparse_idxs[param_index][result_index].squeeze(dim=0),
                    sparse_vals[param_index][result_index].squeeze(dim=0),
                    xshape,
                    totalk,
                )
            )
        results.append(result)

    return results


def receive_batch(
    device: torch.device,
    batch_shape: list[int],
    batch_has_labels: bool,
    batch_has_position_ids: bool,
) -> tuple[torch.Tensor, torch.Tensor | None, torch.Tensor | None]:
    input_ids = torch.empty(batch_shape, dtype=torch.long, device=device)
    labels = (
        torch.empty(batch_shape, dtype=torch.long, device=device)
        if batch_has_labels
        else None
    )
    position_ids = (
        torch.empty(batch_shape, dtype=torch.long, device=device)
        if batch_has_position_ids
        else None
    )
    dist.broadcast(input_ids, 0)
    if batch_has_labels:
        dist.broadcast(labels, 0)
    if batch_has_position_ids:
        dist.broadcast(position_ids, 0)
    return (input_ids, labels, position_ids)


def main():
    parser = argparse.ArgumentParser()

    parser.add_argument("--parent-pid", type=int)
    parser.add_argument("--backend", type=str)
    parser.add_argument("--init-method", type=str)
    parser.add_argument("--world-size", type=int)
    parser.add_argument("--rank", type=int, required=True)
    parser.add_argument(
        "--device",
        type=int,
    )

    args = parser.parse_args()

    if args.parent_pid:
        start_process_watcher(args.parent_pid, timedelta(seconds=1))

    torch.manual_seed(1337)

    # parse init_method to manually create TCP store
    store = None
    if args.init_method.startswith("tcp://"):
        host_name, port = args.init_method[6:].split(":")
        store = dist.TCPStore(
            host_name=host_name,
            port=int(port),
            world_size=args.world_size,
            is_master=False,
            timeout=timedelta(hours=2),
            use_libuv=True,
        )

    dist.init_process_group(
        backend=args.backend,
        init_method=args.init_method if store is None else None,
        timeout=timedelta(hours=2),
        world_size=args.world_size,
        rank=args.rank if args.world_size else None,
        store=store,
    )

    def barrier():
        dist.barrier(device_ids=[args.device] if args.device is not None else None)

    architecture = store.get("architecture").decode()
    source = store.get("source").decode()
    if source == "files":
        files = store.get("files").decode()
        files_list = json.loads(files)

        # Expand ~/ to the actual home directory on this machine
        expanded_files = [os.path.expanduser(file_path) for file_path in files_list]
        source = PretrainedSourceRepoFiles(files=expanded_files)
    elif source == "config_and_tensors":
        # Sync all ranks before receiving anything
        barrier()
        config = store.get("config").decode()
        tensor_names = json.loads(store.get("tensor_names").decode())
        state_dict = {}

        for name in tensor_names:
            # Get metadata for this tensor
            tensor_shape = json.loads(store.get(f"tensor_shape_{name}").decode())
            tensor_dtype_str = store.get(f"tensor_dtype_{name}").decode()

            # Map Rust dtype string to PyTorch dtype
            dtype_map = {
                "Float": torch.float32,
                "Double": torch.float64,
                "Int": torch.int32,
                "Int64": torch.int64,
                "Half": torch.float16,
                "BFloat16": torch.bfloat16,
            }
            tensor_dtype = dtype_map.get(tensor_dtype_str, torch.float32)

            # Create empty tensor to overwrite with the broadcasted tensor
            tensor = torch.empty(tensor_shape, dtype=tensor_dtype, device=args.device)

            dist.broadcast(tensor, 0)
            barrier()

            state_dict[name] = (
                tensor.cpu()
            )  # move back to CPU memory so we don't hold full model in GPU memory

        source = PretrainedSourceStateDict(config_json=config, state_dict=state_dict)
    else:
        raise ValueError(f"Unsupported source type {source}")

    dp = int(store.get("dp").decode())
    tp = int(store.get("tp").decode())

    device = args.device if args.device else 0

    model = make_causal_lm(
        architecture,
        source,
        device,
        attn_implementation="flash_attention_2",
        dp=dp,
        tp=tp,
    )

    trainer: Optional[Trainer] = None
    iteration = 0

    while True:
        try:
            operation = store.get(str(iteration))
        except:
            return
        operation = json.loads(operation.decode())

        barrier()

        if operation["operation"] == "hyperparameters":
            hyperparameters: Hyperparameters = Hyperparameters(**operation)

            if hyperparameters.grad_accum_in_fp32:
                raise ValueError("FP32 reduce not supported in Python Hf yet")

            trainer = Trainer(
                device,
                model,
                json.dumps(hyperparameters.lr_scheduler),
                json.dumps(hyperparameters.optimizer),
                json.dumps(model.get_config()),
                hyperparameters.micro_batch_size,
                hyperparameters.grad_accum_in_fp32,
            )
        elif operation["operation"] == "train":
            if trainer is None:
                raise RuntimeError(
                    "Got train operation without having created a trainer"
                )

            train = TrainOperation(**operation)
            prev_self_distro_results = []
            if train.results_len > 0 and train.results_metadata:
                prev_self_distro_results = receive_distro_results(
                    train.results_len,
                    DistroResultsMetadata(**train.results_metadata),
                    device=device,
                )

            input_ids, labels, position_ids = receive_batch(
                device,
                train.batch_shape,
                train.batch_has_labels,
                train.batch_has_position_ids,
            )

            _, loss = trainer.train(
                train.step,
                train.zero_optim,
                (train.batch_id[0], train.batch_id[1]),
                input_ids,
                labels,
                position_ids,
                train.batch_sequence_lengths,
                (
                    (train.warmup_lr_between[0], train.warmup_lr_between[1])
                    if train.warmup_lr_between is not None
                    else None
                ),
                prev_self_distro_results,
            )

            loss = torch.Tensor([loss]).to(device=device, dtype=torch.float32)
            dist.all_reduce(loss)
        elif operation["operation"] == "optimize":
            if trainer is None:
                raise RuntimeError(
                    "Got train operation without having created a trainer"
                )

            with torch.no_grad():
                optimize = OptimizeOperation(**operation)

                results = []
                if optimize.results_len > 0 and optimize.results_metadata:
                    results = receive_distro_results(
                        optimize.results_len,
                        DistroResultsMetadata(**optimize.results_metadata),
                        device=device,
                    )

                trainer.optimize(
                    optimize.step,
                    (
                        (optimize.warmup_lr_between[0], optimize.warmup_lr_between[1])
                        if optimize.warmup_lr_between is not None
                        else None
                    ),
                    results,
                )
        elif operation["operation"] == "extract":
            if trainer is None:
                raise RuntimeError(
                    "Got train operation without having created a trainer"
                )

            with torch.no_grad():
                trainer.extract()
        elif operation["operation"] == "forward":
            with torch.no_grad():
                forward = ForwardOperation(**operation)

                input_ids, labels, position_ids = receive_batch(
                    device,
                    forward.batch_shape,
                    forward.batch_has_labels,
                    forward.batch_has_position_ids,
                )

                model.forward(
                    input_ids=input_ids,
                    labels=labels,
                    position_ids=position_ids,
                    sequence_lengths=forward.batch_sequence_lengths,
                    num_logits_to_keep=forward.num_logits_to_keep,
                    loss_scale=forward.loss_scale,
                )
        elif operation["operation"] == "exit":
            return

        iteration += 1


main()
