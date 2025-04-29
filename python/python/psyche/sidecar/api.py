from dataclasses import dataclass


@dataclass
class Hyperparameters:
    lr_scheduler: dict
    optimizer: dict
    micro_batch_size: int
    grad_accum_in_fp32: bool


@dataclass
class Operation:
    operation: str


@dataclass
class DistroResultsMetadata:
    sparse_idx_size: list[list[int]]
    sparse_idx_dtype: int
    sparse_val_size: list[list[int]]
    sparse_val_dtype: int
    xshape: list[list[int]]
    totalk: list[int]


@dataclass
class TrainOperation(Operation):
    step: int
    batch_id: tuple[int, int]
    batch_shape: list[int]
    zero_optim: bool
    results_len: int
    warmup_lr_between: tuple[int, int] | None = None
    results_metadata: DistroResultsMetadata | None = None


@dataclass
class OptimizeOperation(Operation):
    step: int
    results_len: int
    warmup_lr_between: tuple[int, int] | None = None
    results_metadata: DistroResultsMetadata | None = None
