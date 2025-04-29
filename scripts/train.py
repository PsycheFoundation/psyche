import numpy as np
import time
from numba import jit
from numba.typed import List as TypedList
import os
import torch
import torch.distributed as dist
from torch.nn.parallel import DistributedDataParallel
from torch.distributed.fsdp import (
    FullyShardedDataParallel as FSDPv2,
    MixedPrecision,
)
from torch.distributed.fsdp.wrap import (
    transformer_auto_wrap_policy,
)
from transformers import (
    LlamaForCausalLM,
    default_data_collator,
    get_cosine_schedule_with_warmup,
)
from transformers.models.llama.modeling_llama import LlamaDecoderLayer
import argparse
from torch.utils.data import DataLoader
import torch.optim as optim
from torch.optim.lr_scheduler import CosineAnnealingLR
from typing import List, Tuple, Union, Optional
from pathlib import Path


def parse_args():
    parser = argparse.ArgumentParser()
    parser.add_argument("--model", default="emozilla/llama2-215m-init")
    parser.add_argument("--data_path", default="data")
    parser.add_argument("--sequence_length", type=int, default=2048)
    parser.add_argument("--token_size", type=int, default=2)
    parser.add_argument("--micro_batch", type=int, default=8)
    parser.add_argument("--total_batch", type=int, default=64)
    parser.add_argument("--beta1", type=float, default=0.9)
    parser.add_argument("--beta2", type=float, default=0.95)
    parser.add_argument("--weight_decay", type=float, default=0.1)
    parser.add_argument("--eps", type=float, default=1e-8)
    parser.add_argument("--learning_rate", type=float, default=4e-4)
    parser.add_argument("--warmup_steps", type=int, default=500)
    parser.add_argument("--total_steps", type=int, default=25000)
    parser.add_argument("--max_grad_norm", type=float, default=1.0)
    parser.add_argument("--use_fsdp", action="store_true")
    parser.add_argument("--cpu", action="store_true")
    parser.add_argument("--attn_implementation", default="flash_attention_2")
    return parser.parse_args()


def setup_distributed():
    rank = int(os.environ["LOCAL_RANK"])
    dist.init_process_group(
        "nccl",
        rank=rank,
        world_size=int(os.environ["WORLD_SIZE"]),
    )
    torch.cuda.set_device(rank)


def get_model(args):
    model = LlamaForCausalLM.from_pretrained(
        args.model,
        torch_dtype=torch.bfloat16,
        device_map=f"cuda:{int(os.environ['LOCAL_RANK'])}" if not args.cpu else "cpu",
        attn_implementation=args.attn_implementation,
    )

    if args.use_fsdp:
        mixed_precision_policy = MixedPrecision(
            param_dtype=torch.bfloat16,
            reduce_dtype=torch.float32,
        )

        def wrap_policy(module, recurse=True, nonwrapped_numel=-1):
            return transformer_auto_wrap_policy(
                module=module,
                recurse=recurse,
                nonwrapped_numel=nonwrapped_numel,
                transformer_layer_cls={LlamaDecoderLayer},
            )

        model = FSDPv2(
            model,
            mixed_precision=mixed_precision_policy,
            auto_wrap_policy=wrap_policy,
            device_id=torch.cuda.current_device(),
        )
    elif dist.is_initialized():
        model = DistributedDataParallel(
            model,
            device_ids=[torch.cuda.current_device()],
            output_device=torch.cuda.current_device(),
        )

    return model


def get_data_files(data_path):
    data_path = Path(data_path)
    if data_path.is_file():
        return [str(data_path)]

    valid_extensions = {".npy", ".bin", ".ds"}
    data_files = [
        str(f)
        for f in data_path.iterdir()
        if f.is_file() and f.suffix in valid_extensions
    ]

    if not data_files:
        raise ValueError(f"No .npy, .bin, or .ds files found in {data_path}")

    return data_files


def normalize(weights: List[float]) -> List[np.array]:
    w = np.array(weights, dtype=np.float64)
    w_sum = np.sum(w)
    w = w / w_sum
    return w


@jit(nopython=True, cache=True)
def build_nanoset_index_helper(
    n_samples: int, weights: np.ndarray, dataset_sizes: List[int]
) -> Tuple[np.ndarray, np.ndarray]:
    # Create empty arrays for dataset indices and dataset sample indices
    dataset_index = np.empty((n_samples,), dtype="uint")
    dataset_sample_index = np.empty(
        (n_samples,), dtype="long"
    )  # Supports dataset with up to 2**64 samples

    # Initialize buffer for number of samples used for each dataset
    current_samples = np.zeros((len(weights),), dtype="long")

    # Iterate over all samples
    for sample_idx in range(n_samples):

        # Convert sample index to float for comparison against weights
        sample_idx_float = max(sample_idx, 1.0)

        # Find the dataset with the highest error
        errors = weights * sample_idx_float - current_samples
        max_error_index = np.argmax(errors)

        # Assign the dataset index and update the sample index
        dataset_index[sample_idx] = max_error_index
        dataset_sample_index[sample_idx] = (
            current_samples[max_error_index] % dataset_sizes[max_error_index]
        )

        # Update the total samples for the selected dataset
        current_samples[max_error_index] += 1

    return dataset_index, dataset_sample_index


class Nanoset(torch.utils.data.Dataset):
    def __init__(
        self,
        dataset_paths: List[str],
        dataset_weights: Union[List[float], None],
        sequence_length: int,
        token_dtype: Union[np.uint16, np.int32],
        random_seed: Optional[int] = None,
    ) -> None:

        # Init
        self.dataset_paths = dataset_paths
        self.dataset_weights = dataset_weights
        self.sequence_length = sequence_length
        self.token_dtype = token_dtype
        self.random_seed = random_seed
        self.index = 0

    def start(self):
        if self.have_data_ready():
            return

        # Build Nanoset Index
        ## To build the index we need the length of each dataset
        self.dataset_lengths = TypedList()
        for dataset_path in self.dataset_paths:
            self.dataset_buffer_mmap = np.memmap(
                dataset_path, mode="r", order="C", dtype=self.token_dtype
            )
            self.dataset_buffer = memoryview(self.dataset_buffer_mmap)
            dataset_number_of_tokens = int(len(self.dataset_buffer))
            number_of_samples = int(
                (dataset_number_of_tokens - 1) / self.sequence_length
            )  # Discard last sample if length < sequence_length
            self.dataset_lengths.append(number_of_samples)
        ## Set dataset weights
        if (
            self.dataset_weights is None
        ):  # Case of training with > 1 datasets without weighting them: Consume both datasets entirely on each epoch
            self.dataset_weights = normalize(self.dataset_lengths)
        else:
            self.dataset_weights = normalize(self.dataset_weights)

        ## Build dataset index and dataset sample index
        self.dataset_index, self.dataset_sample_index = self.build_nanoset_index()

    def stop(self):
        pass

    def have_data_ready(self) -> bool:
        return hasattr(self, "dataset_index")

    def __len__(self) -> int:
        return len(self.dataset_index) if self.have_data_ready() else 0

    def __getitem__(self, idx: int) -> np.ndarray:
        if not self.have_data_ready():
            raise RuntimeError("Data not ready")

        dataset = self.dataset_index[idx]
        dataset_sample = self.dataset_sample_index[idx]

        # Rebuild the memmap in every access to free memory
        # https://stackoverflow.com/a/61472122
        self.dataset_buffer_mmap = np.memmap(
            self.dataset_paths[dataset], mode="r", order="C", dtype=self.token_dtype
        )
        self.dataset_buffer = memoryview(self.dataset_buffer_mmap)

        # uint16 -> 2 bytes per token, int32 -> 4 bytes per token
        offset = (
            int(dataset_sample)
            * self.sequence_length
            * int(np.iinfo(self.token_dtype).bits / 8)
        )
        input_ids_tokens = np.frombuffer(
            self.dataset_buffer,
            dtype=self.token_dtype,
            count=self.sequence_length + 1,
            offset=offset,
        )

        # Return tokens as np.int32 as Torch can't handle uint16
        return {"labels": input_ids_tokens.astype(np.int32)}

    def build_nanoset_index(self) -> np.ndarray:
        # Compute samples per epoch and number of epochs
        samples_per_epoch = sum(self.dataset_lengths)
        # num_epochs = int(self.train_split_num_samples / samples_per_epoch) + 1
        num_epochs = 1
        # Build the dataset indexes for 1 epoch
        dataset_index, dataset_sample_index = build_nanoset_index_helper(
            n_samples=samples_per_epoch,
            weights=self.dataset_weights,
            dataset_sizes=self.dataset_lengths,
        )
        if self.random_seed is not None:
            # Shuffle the indexes the same way
            numpy_random_state = np.random.RandomState(self.random_seed)
            numpy_random_state.shuffle(dataset_index)
            numpy_random_state = np.random.RandomState(self.random_seed)
            numpy_random_state.shuffle(dataset_sample_index)
            # Concatenate num_epochs the shuffled indexes
        dataset_index = np.concatenate([dataset_index for _ in range(num_epochs)])
        dataset_sample_index = np.concatenate(
            [dataset_sample_index for _ in range(num_epochs)]
        )
        # Just keep the necessary samples
        # dataset_index = dataset_index[: self.train_split_num_samples]
        # dataset_sample_index = dataset_sample_index[: self.train_split_num_samples]

        return dataset_index, dataset_sample_index

    def __del__(self) -> None:
        if hasattr(self, "dataset_buffer_mmap"):
            self.dataset_buffer_mmap._mmap.close()
        del self.dataset_buffer_mmap

    def get_data_offset(self, split: str = "train") -> Optional[int]:
        if split != "train":
            raise ValueError("Data offset only supported for train split")
        return self.index

    def get_seed(self) -> Optional[int]:
        return self.random_seed


def train(args):
    if args.use_fsdp or not args.cpu:
        setup_distributed()

    # Setup dataset
    data_files = get_data_files(args.data_path)
    dataset = Nanoset(
        data_files,
        None,
        args.sequence_length,
        np.int16 if args.token_size == 2 else np.int32,
    )
    dataset.start()

    dataloader = DataLoader(
        dataset,
        batch_size=args.micro_batch,
        collate_fn=default_data_collator,
    )

    model = get_model(args)

    optimizer = optim.AdamW(
        model.parameters(),
        lr=args.learning_rate,
        betas=(args.beta1, args.beta2),
        eps=args.eps,
        weight_decay=args.weight_decay,
    )

    scheduler = get_cosine_schedule_with_warmup(
        optimizer, args.warmup_steps, args.total_steps
    )

    grad_accum_steps = args.total_batch // args.micro_batch
    model.train()

    data = iter(dataloader)
    for step in range(args.total_steps):
        start_time = time.time()
        avg_loss = 0.0
        optimizer.zero_grad()

        for _ in range(grad_accum_steps):
            batch = next(data)
            labels = batch["labels"].to(device=model.device, dtype=torch.int64)

            outputs = model(
                input_ids=labels,
                labels=labels.clone(),
            )

            loss = outputs.loss / grad_accum_steps
            loss.backward()

            avg_loss += loss.item()

        torch.nn.utils.clip_grad_norm_(model.parameters(), args.max_grad_norm)

        optimizer.step()
        scheduler.step()

        duration = time.time() - start_time

        if dist.get_rank() == 0:
            print(
                f"step: {step}, "
                f"duration: {duration:.1f}, "
                f"lr: {scheduler.get_last_lr()[0]:.1e}, "
                f"loss: {avg_loss:.4f}"
            )


if __name__ == "__main__":
    args = parse_args()
    train(args)
