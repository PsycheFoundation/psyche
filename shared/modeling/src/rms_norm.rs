use tch::{
    Kind, Tensor,
    nn::{self, Module},
};

#[derive(Debug)]
pub struct RMSNorm {
    weight: Tensor,
    eps: f64,
}

impl RMSNorm {
    pub fn new(vs: nn::Path, size: i64, eps: f64) -> Self {
        let weight = vs.ones("weight", &[size]);
        Self { weight, eps }
    }
}

impl Module for RMSNorm {
    fn forward(&self, xs: &Tensor) -> Tensor {
        let kind = xs.kind();
        let xs = xs.to_kind(Kind::Float);
        let variance = xs.pow_tensor_scalar(2).mean_dim(-1, true, Kind::Float);
        let xs_normed = xs * (variance + self.eps).rsqrt();
        let xs_normed = xs_normed.to_kind(kind);
        &self.weight * xs_normed
    }
}
