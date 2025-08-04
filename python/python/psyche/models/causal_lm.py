import torch

from abc import ABC, abstractmethod
from dataclasses import dataclass
from typing import Optional, Tuple, Union, Iterable


@dataclass
class PretrainedSourceRepoFiles:
    files: list[str]


@dataclass
class PretrainedSourceStateDict:
    config_json: str
    state_dict: dict[str, torch.Tensor]


class CausalLM(ABC):

    @staticmethod
    @abstractmethod
    def from_pretrained(
        source: PretrainedSourceRepoFiles | PretrainedSourceStateDict,
        device: torch.device,
        dp: int = 1,
        tp: int = 1,
        param_dtype: torch.dtype = torch.bfloat16,
        reduce_dtype: torch.dtype = torch.float32,
        fsdp_modules: Optional[Iterable[str]] = None,
    ):
        pass

    @abstractmethod
    def forward(
        self,
        input_ids: torch.Tensor,
        labels: Optional[torch.Tensor],
        num_logits_to_keep: Optional[int] = 0,
        loss_scale: Optional[float] = None,
    ) -> Tuple[torch.Tensor, Optional[torch.Tensor]]:
        pass

    @abstractmethod
    def named_parameters(self) -> dict[str, torch.Tensor]:
        pass

    @abstractmethod
    def get_config(self) -> dict:
        pass
