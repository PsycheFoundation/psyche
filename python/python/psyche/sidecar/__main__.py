import argparse
import torch
import json
import torch.distributed as dist
import psutil

from .. import make_causal_lm, PretrainedSourceRepoFiles, Trainer, DistroResult


def main():
    parser = argparse.ArgumentParser()

    parser.add_argument("--parent-pid", type=int, required=True)
    parser.add_argument("--backend", type=str)
    parser.add_argument("--init-method", type=str)
    parser.add_argument("--world-size", type=int)
    parser.add_argument("--rank", type=int, required=True)

    args = parser.parse_args()

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

    model = make_causal_lm(architecture, source, args.rank, dp=dp, tp=tp)

    store.wait(
        [
            "lr_scheduler_json",
            "optimizer_json",
            "micro_batch_size",
            "grad_accum_in_fp32",
        ]
    )
    lr_scheduler_json = store.get("lr_scheduler_json").decode()
    optimizer_json = store.get("optimizer_json").decode()
    micro_batch_size = int(store.get("micro_batch_size").decode())
    grad_accum_in_fp32 = (
        False if int(store.get("grad_accum_in_fp32").decode()) == 0 else True
    )

    trainer = Trainer(
        args.rank,
        model,
        lr_scheduler_json,
        optimizer_json,
        json.dumps(model.get_config()),
        micro_batch_size,
        grad_accum_in_fp32,
    )

    parent_process = psutil.Process(args.parent_pid)
    iteration = 0
    last_result = None
    while parent_process.is_running():
        store.wait([str(iteration)])
        operation = store.get(str(iteration)).decode()

        if operation == "train":
            batch_shape = [int(x) for x in store.get("batch-shape").decode().split(",")]
            batch = torch.empty(batch_shape, dtype=torch.int, device=args.rank)
            dist.broadcast(batch, 0)
            step = int(store.get("step").decode())
            batch_id = int(store.get("batch-id").decode())
            last_result = trainer.train(step, batch_id, batch)
        elif operation.startswith("optimize"):
            with torch.no_grad():
                step = int(store.get("step").decode())
                if operation.endswith("-distro") and last_result is not None:
                    results_len = int(store.get("results-len").decode())
                    sparse_idxs = []
                    sparse_vals = []
                    for param_index in range(len(last_result)):
                        sparse_idx = last_result[param_index].sparse_idx
                        sparse_val = last_result[param_index].sparse_val
                        sparse_idx_size = (results_len,) + tuple(sparse_idx.size())
                        sparse_val_size = (results_len,) + tuple(sparse_val.size())

                        sparse_idx = torch.empty(
                            sparse_idx_size,
                            dtype=sparse_idx.dtype,
                            layout=sparse_idx.layout,
                            device=sparse_idx.device,
                        )
                        sparse_val = torch.empty(
                            sparse_val_size,
                            dtype=sparse_val.dtype,
                            layout=sparse_val.layout,
                            device=sparse_val.device,
                        )
                        dist.broadcast(sparse_idx, 0)
                        dist.broadcast(sparse_val, 0)

                        sparse_idxs.append(sparse_idx.chunk(results_len, dim=0))
                        sparse_vals.append(sparse_val.chunk(results_len, dim=0))

                    results = []
                    for result_index in range(results_len):
                        result = []
                        for param_index in range(len(last_result)):
                            xshape = last_result[param_index].xshape
                            totalk = last_result[param_index].totalk
                            result.append(
                                DistroResult(
                                    sparse_idxs[param_index][result_index].squeeze(dim=0),
                                    sparse_vals[param_index][result_index].squeeze(dim=0),
                                    xshape,
                                    totalk,
                                )
                            )
                        results.append(result)
                    trainer.optimize(step, results)
                else:
                    trainer.optimize(step)
        elif operation == "extract":
            trainer.extract()

        iteration += 1


main()