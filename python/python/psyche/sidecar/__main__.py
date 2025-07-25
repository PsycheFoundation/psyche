import argparse
import torch
import json
import torch.distributed as dist

from datetime import timedelta
from .. import (
    make_causal_lm,
    PretrainedSourceRepoFiles,
    Trainer,
    DistroResult,
    start_process_watcher,
)
from .api import (
    DistroResultsMetadata,
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


def main():
    parser = argparse.ArgumentParser()

    parser.add_argument("--parent-pid", type=int)
    parser.add_argument("--backend", type=str)
    parser.add_argument("--init-method", type=str)
    parser.add_argument("--world-size", type=int)
    parser.add_argument("--rank", type=int, required=True)

    args = parser.parse_args()

    if args.parent_pid:
        start_process_watcher(args.parent_pid, timedelta(seconds=1))

    dist.init_process_group(
        backend=args.backend,
        init_method=args.init_method,
        world_size=args.world_size,
        rank=args.rank if args.world_size else None,
    )

    store = dist.distributed_c10d._get_default_store()

    store.wait(["architecture", "source"])
    architecture = store.get("architecture").decode()
    source = store.get("source").decode()
    if source == "files":
        store.wait(["files"])
        files = store.get("files").decode()
        source = PretrainedSourceRepoFiles(files=json.loads(files))
    else:
        raise ValueError(f"Unsupported source type {source}")

    store.wait(["dp", "tp"])
    dp = int(store.get("dp").decode())
    tp = int(store.get("tp").decode())

    device = torch.device(args.rank)
    model = make_causal_lm(
        architecture,
        source,
        device,
        dp=dp,
        tp=tp,
    )

    store.wait(["hyperparameters"])
    hyperparameters: Hyperparameters = Hyperparameters(
        **json.loads(store.get("hyperparameters").decode())
    )

    if hyperparameters.grad_accum_in_fp32:
        raise RuntimeError("FP32 reduce not supported in Python Hf yet")

    trainer = Trainer(
        args.rank,
        model,
        json.dumps(hyperparameters.lr_scheduler),
        json.dumps(hyperparameters.optimizer),
        json.dumps(model.get_config()),
        hyperparameters.micro_batch_size,
        hyperparameters.grad_accum_in_fp32,
    )

    iteration = 0

    while True:
        store.wait([str(iteration)])
        operation = json.loads(store.get(str(iteration)).decode())

        if operation["operation"] == "train":
            train = TrainOperation(**operation)

            prev_self_distro_results = []
            if train.results_len > 0 and train.results_metadata:
                prev_self_distro_results = receive_distro_results(
                    train.results_len,
                    DistroResultsMetadata(**train.results_metadata),
                    device=device,
                )

            batch = torch.empty(train.batch_shape, dtype=torch.long, device=device)
            dist.broadcast(batch, 0)

            # world_size = dist.get_world_size()
            # rank = dist.get_rank()
            # shard_size = batch.shape[0] // world_size
            # start_row = rank * shard_size
            # local_batch = batch.narrow(0, start_row, shard_size).contiguous()

            trainer.train(
                train.step,
                (train.batch_id[0], train.batch_id[1]),
                batch,
                train.zero_optim,
                (
                    (train.warmup_lr_between[0], train.warmup_lr_between[1])
                    if train.warmup_lr_between is not None
                    else None
                ),
                prev_self_distro_results,
            )
        elif operation["operation"] == "optimize":
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
            with torch.no_grad():
                trainer.extract()

        iteration += 1


main()
