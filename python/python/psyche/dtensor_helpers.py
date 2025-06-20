from typing import List
from torch import Tensor
from torch.distributed.tensor import DTensor, distribute_tensor, zeros


def gather_full_tensor(dtensor: DTensor) -> Tensor:
    return dtensor.full_tensor()


def calculate_local_tensor_from_full(tensor: Tensor, like: DTensor) -> Tensor:
    return distribute_tensor(
        tensor, device_mesh=like.device_mesh, placements=like.placements
    )


def full_tensor_shape(dtensor: DTensor) -> List[int]:
    return dtensor.shape


def local_tensor(dtensor: DTensor) -> Tensor:
    return dtensor.to_local()


def zeros_like(dtensor: DTensor) -> DTensor:
    return zeros(
        dtensor.shape,
        dtype=dtensor.dtype,
        layout=dtensor.layout,
        device_mesh=dtensor.device_mesh,
        placements=dtensor.placements,
    )


def set_grad(tensor: Tensor | DTensor, grad: Tensor):
    if isinstance(tensor, DTensor):
        grad = distribute_tensor(
            grad, device_mesh=tensor.grad.device_mesh, placements=tensor.grad.placements
        )
        tensor.grad._local_tensor.copy_(grad._local_tensor)
    else:
        tensor.grad.copy_(grad)
