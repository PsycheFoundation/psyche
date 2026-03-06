use tch::{Device, Tensor};

/// Wrapper around Tensor::cat. For Linux/CUDA this is the same as Tensor::cat,
/// but for MPS, we need to move the tensors to CPU before concatenating and then move back to MPS,
/// else it would crash.
pub fn cat(tensors: &[&Tensor], dim: i64) -> Tensor {
    if tensors.is_empty() {
        return Tensor::empty([0], (tch::Kind::Float, Device::Cpu));
    }
    let device = tensors[0].device();
    if device != Device::Mps {
        return Tensor::cat(tensors, dim);
    }
    let cpu_tensors: Vec<Tensor> = tensors.iter().map(|t| t.to(Device::Cpu)).collect();
    Tensor::cat(&cpu_tensors.iter().collect::<Vec<_>>(), dim).to(device)
}

/// Owned-tensor variant for when callers already have Vec<Tensor>.
pub fn cat_owned(tensors: &[Tensor], dim: i64) -> Tensor {
    if tensors.is_empty() {
        return Tensor::empty([0], (tch::Kind::Float, Device::Cpu));
    }
    let device = tensors[0].device();
    if device != Device::Mps {
        return Tensor::cat(tensors, dim);
    }
    let cpu_tensors: Vec<Tensor> = tensors.iter().map(|t| t.to(Device::Cpu)).collect();
    Tensor::cat(&cpu_tensors.iter().collect::<Vec<_>>(), dim).to(device)
}
