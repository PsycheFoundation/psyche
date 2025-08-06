use std::f32::consts::PI;

use tch::{Device, Kind, Tensor};

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, Default, PartialEq)]
pub enum RoPEType {
    #[serde(rename = "llama3")]
    Llama3,
    #[default]
    #[serde(rename = "default")]
    Default,
    #[serde(rename = "yarn")]
    YaRN,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, Default)]
pub struct RoPEConfig {
    pub factor: Option<f32>,
    pub low_freq_factor: Option<f32>,
    pub high_freq_factor: Option<f32>,
    pub original_max_position_embeddings: Option<usize>,
    #[serde(alias = "type")]
    pub rope_type: RoPEType,
    pub beta_fast: Option<f32>,
    pub beta_slow: Option<f32>,
    pub mscale: Option<f32>,
    pub mscale_all_dim: Option<f32>,
}

pub fn default_rope() -> f32 {
    10_000.0
}

fn calculate_default_inv_freq(head_dim: usize, rope_theta: f32) -> Vec<f32> {
    (0..head_dim)
        .step_by(2)
        .map(|i| 1f32 / rope_theta.powf(i as f32 / head_dim as f32))
        .collect()
}

fn yarn_find_correction_dim(
    num_rotations: f32,
    dim: usize,
    base: f32,
    max_position_embeddings: usize,
) -> f32 {
    (dim as f32
        * (max_position_embeddings as f32 / (num_rotations * 2.0 * std::f32::consts::PI)).ln())
        / (2.0 * base.ln())
}

fn yarn_find_correction_range(
    low_rot: f32,
    high_rot: f32,
    dim: usize,
    base: f32,
    max_position_embeddings: usize,
) -> (usize, usize) {
    let low =
        yarn_find_correction_dim(low_rot, dim, base, max_position_embeddings).floor() as usize;
    let high =
        yarn_find_correction_dim(high_rot, dim, base, max_position_embeddings).ceil() as usize;
    (low.max(0), high.min(dim - 1))
}

fn yarn_linear_ramp_mask(min: usize, max: usize, dim: usize, device: Device) -> Tensor {
    let max = if min == max { max + 1 } else { max }; // Prevent singularity
    let t = Tensor::arange(dim as i64, (Kind::Float, device));
    let linear_func = (&t - min as f64) / (max as f64 - min as f64);
    linear_func.clamp(0.0, 1.0)
}

pub fn yarn_get_mscale(scale: f32, mscale: f32) -> f32 {
    if scale <= 1.0 {
        1.0
    } else {
        0.1 * mscale * scale.ln() + 1.0
    }
}

#[derive(Debug)]
pub struct RoPECache {
    pub inv_freq: Tensor,
    pub mscale: f64,
}

impl RoPECache {
    pub fn new(
        rope_config: &Option<RoPEConfig>,
        head_dim: usize,
        rope_theta: f32,
        device: &Device,
    ) -> Self {
        let inv_freq = calculate_default_inv_freq(head_dim, rope_theta);

        let (inv_freq, mscale) = match rope_config {
            None
            | Some(RoPEConfig {
                rope_type: RoPEType::Default,
                ..
            }) => (Tensor::from_slice(&inv_freq).to(*device), None),
            Some(RoPEConfig {
                rope_type: RoPEType::Llama3,
                original_max_position_embeddings,
                factor,
                low_freq_factor,
                high_freq_factor,
                ..
            }) => {
                let original_max_position_embeddings =
                    original_max_position_embeddings.unwrap() as f32;
                let factor = factor.unwrap();
                let low_freq_factor = low_freq_factor.unwrap();
                let high_freq_factor = high_freq_factor.unwrap();
                let low_freq_wavelen = original_max_position_embeddings / low_freq_factor;
                let high_freq_wavelen = original_max_position_embeddings / high_freq_factor;

                let inv_freq = inv_freq
                    .into_iter()
                    .map(|freq| {
                        let wavelen = 2. * PI / freq;
                        if wavelen < high_freq_wavelen {
                            freq
                        } else if wavelen > low_freq_wavelen {
                            freq / factor
                        } else {
                            let smooth = (original_max_position_embeddings / wavelen
                                - low_freq_factor)
                                / (high_freq_factor - low_freq_factor);
                            (1. - smooth) * freq / factor + smooth * freq
                        }
                    })
                    .collect::<Vec<_>>();

                (Tensor::from_slice(&inv_freq).to(*device), None)
            }
            Some(RoPEConfig {
                rope_type: RoPEType::YaRN,
                factor,
                beta_fast,
                beta_slow,
                original_max_position_embeddings,
                mscale,
                mscale_all_dim,
                ..
            }) => {
                let freq_extra = Tensor::from_slice(&inv_freq).to(*device);

                let theta_inter =
                    calculate_default_inv_freq(head_dim, rope_theta * factor.unwrap());
                let freq_inter = Tensor::from_slice(&theta_inter).to(*device);

                let (low, high) = yarn_find_correction_range(
                    beta_fast.unwrap(),
                    beta_slow.unwrap(),
                    head_dim,
                    rope_theta,
                    original_max_position_embeddings.unwrap(),
                );

                // Create interpolation mask
                let inv_freq_mask = 1.0 - yarn_linear_ramp_mask(low, high, head_dim / 2, *device);

                let inv_freq = &freq_inter * (1.0 - &inv_freq_mask) + &freq_extra * &inv_freq_mask;

                // Calculate scaling factor
                let mscale = yarn_get_mscale(factor.unwrap(), mscale.unwrap())
                    / yarn_get_mscale(factor.unwrap(), mscale_all_dim.unwrap_or(1.));

                (inv_freq, Some(mscale as f64))
            }
        };

        Self {
            inv_freq,
            mscale: mscale.unwrap_or(1.0),
        }
    }
}

pub fn rotate_half(xs: &Tensor) -> Tensor {
    let last_dim = *xs.size().last().unwrap();
    let xs1 = xs.narrow(-1, 0, last_dim / 2);
    let xs2 = xs.narrow(-1, last_dim / 2, last_dim - last_dim / 2);
    Tensor::cat(&[&xs2.neg(), &xs1], -1)
}

impl RoPECache {
    pub fn apply_rotary_emb(&self, x: &Tensor, position_ids: Option<&Tensor>) -> Tensor {
        let (b_sz, _, seq_len, _) = x.size4().unwrap();
        let position_ids = match position_ids {
            Some(ids) => ids,
            None => {
                // Create default sequential position_ids starting from 0
                &Tensor::arange(seq_len, (Kind::Int64, x.device()))
                    .unsqueeze(0)
                    .expand([b_sz, seq_len], false)
            }
        };
        let pos_shape = position_ids.size();
        assert_eq!(
            pos_shape.len(),
            2,
            "position_ids must be 2D [batch, seq_len]"
        );
        let pos_b = pos_shape[0];
        let pos_seq = pos_shape[1];
        assert_eq!(
            pos_seq, seq_len,
            "sequence length mismatch between x and position_ids"
        );
        // If position_ids batch is 1, it will broadcast; otherwise, must match b_sz
        assert!(
            pos_b == 1 || pos_b == b_sz,
            "batch size mismatch between position_ids and x"
        );

        let head_dim_2 = self.inv_freq.size()[0];
        let inv_freq_expanded = self
            .inv_freq
            .to_kind(Kind::Float)
            .unsqueeze(0)
            .unsqueeze(-1)
            .expand([pos_b, head_dim_2, 1], true);
        let position_ids_expanded = position_ids.to_kind(Kind::Float).unsqueeze(1); // [pos_b, 1, seq_len]

        let freqs = inv_freq_expanded.matmul(&position_ids_expanded); // [pos_b, head_dim_2, seq_len]
        let freqs = freqs.transpose(1, 2); // [pos_b, seq_len, head_dim_2]

        let emb = Tensor::cat(&[&freqs, &freqs], -1); // [pos_b, seq_len, head_dim]

        let mut cos = emb.cos();
        let mut sin = emb.sin();

        if self.mscale != 1.0 {
            let _ = cos.g_mul_scalar_(self.mscale);
            let _ = sin.g_mul_scalar_(self.mscale);
        }

        let cos = cos.unsqueeze(1); // [pos_b, 1, seq_len, head_dim]
        let sin = sin.unsqueeze(1); // [pos_b, 1, seq_len, head_dim]

        let x_kind = x.kind();
        let cos = cos.to_kind(x_kind);
        let sin = sin.to_kind(x_kind);

        (x * &cos) + (rotate_half(x) * &sin)
    }
}
