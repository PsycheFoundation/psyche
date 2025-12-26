from torch import nn
from torch.distributed.checkpoint import HuggingFaceStorageWriter
from transformers import AutoConfig, AutoTokenizer
from huggingface_hub import HfApi
from torchtitan.config import JobConfig
from psyche.models.hf_transformers import auto_config_from_dict
from psyche.models.ttitan import TorchtitanAuto, TRAIN_SPEC_FN

import argparse
import torch
import json
import torch.distributed.checkpoint as dcp
import shutil
import os


def main(args):
    if not args.config:
        raise RuntimeError("No config provided")
    if args.repo and not args.save:
        raise ValueError("`--save` must be used in conjunction with `--repo`")

    config = auto_config_from_dict(json.load(open(args.config)))
    config_tt = TorchtitanAuto.convert_config(config)

    job_config = JobConfig()
    job_config.training.seq_len = config_tt.max_seq_len
    config_tt.update_from_config(job_config)

    if config.model_type not in TRAIN_SPEC_FN:
        raise ValueError(f"Unsupported model_type `{config.model_type}`")
    train_spec = TRAIN_SPEC_FN[config.model_type]()

    torch.set_default_dtype(torch.float32)
    if args.device:
        torch.set_default_device(args.device)

    model = train_spec.model_cls(config_tt)
    with torch.no_grad():
        model.init_weights(buffer_device=None)

    model_param_count, _ = config_tt.get_nparams_and_flops(model, config_tt.max_seq_len)

    print(
        f"created `{config.model_type}`, size: {model_param_count:,} total parameters"
    )

    if args.save:
        sd_adapter = train_spec.state_dict_adapter(config_tt, hf_assets_path=None)

        state_dict = model.state_dict()
        del model

        hf_state_dict = {
            k: v.to(args.dtype) for k, v in sd_adapter.to_hf(state_dict).items()
        }
        storage_writer = HuggingFaceStorageWriter(path=args.save)
        dcp.save(hf_state_dict, storage_writer=storage_writer, checkpoint_id=args.save)

        shutil.copyfile(args.config, os.path.join(args.save, "config.json"))

        if args.tokenizer:
            AutoTokenizer.from_pretrained(args.tokenizer).save_pretrained(args.save)
        if args.repo:
            api = HfApi()
            api.create_repo(
                repo_id=args.repo, private=True, repo_type="model", exist_ok=True
            )
            api.upload_folder(
                folder_path=args.save, repo_id=args.repo, repo_type="model"
            )


args = argparse.ArgumentParser()
args.add_argument(
    "--config",
    type=str,
    help="source config repo or path to JSON config",
)
args.add_argument("--repo", type=str, help="destination repo")
args.add_argument("--save", type=str, help="save to local")
args.add_argument("--dtype", type=int, default=torch.bfloat16, help="torch dtype")
args.add_argument("--private", action="store_true", help="push as a private repo")
args.add_argument("--device", type=str, help="device to init on")
args.add_argument("--tokenizer", type=str, help="tokenizer")

main(args.parse_args())
