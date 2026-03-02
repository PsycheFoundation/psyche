use anchor_lang::{AnchorDeserialize, AnchorSerialize, InitSpace, prelude::borsh};
use bytemuck::Zeroable;
use serde::{Deserialize, Serialize};
use std::f64::consts::PI;
use ts_rs::TS;

pub trait LearningRateScheduler: Send + Sync {
    // lr calculation (especially cosine) is sensitive to fp accuracy
    fn get_lr(&self, step: u32) -> f64;
}

#[derive(
    AnchorSerialize,
    Default,
    AnchorDeserialize,
    InitSpace,
    Serialize,
    Deserialize,
    Clone,
    Debug,
    Zeroable,
    Copy,
    TS,
)]
#[repr(C)]
pub struct ConstantLR {
    base_lr: f64,
    warmup_init_lr: f64,
    warmup_steps: u32,
}

impl ConstantLR {
    #[allow(dead_code)]
    pub fn new(base_lr: f64, warmup_steps: u32, warmup_init_lr: f64) -> Self {
        ConstantLR {
            base_lr,
            warmup_steps,
            warmup_init_lr,
        }
    }

    pub fn get_warmup_steps(&self) -> u32 {
        self.warmup_steps
    }

    pub fn get_warmup_init_lr(&self) -> f64 {
        self.warmup_init_lr
    }
}

impl LearningRateScheduler for ConstantLR {
    fn get_lr(&self, step: u32) -> f64 {
        if step < self.warmup_steps {
            self.warmup_init_lr
                + (self.base_lr - self.warmup_init_lr) * (step as f64 / self.warmup_steps as f64)
        } else {
            self.base_lr
        }
    }
}

#[derive(
    AnchorSerialize,
    AnchorDeserialize,
    InitSpace,
    Serialize,
    Deserialize,
    Clone,
    Debug,
    Zeroable,
    Copy,
    TS,
)]
#[repr(C)]
pub struct LinearLR {
    base_lr: f64,
    warmup_init_lr: f64,
    final_lr: f64,
    warmup_steps: u32,
    total_steps: u32,
}

impl LinearLR {
    #[allow(dead_code)]
    pub fn new(
        base_lr: f64,
        warmup_steps: u32,
        warmup_init_lr: f64,
        total_steps: u32,
        final_lr: f64,
    ) -> Self {
        LinearLR {
            base_lr,
            warmup_steps,
            warmup_init_lr,
            total_steps,
            final_lr,
        }
    }

    pub fn get_warmup_steps(&self) -> u32 {
        self.warmup_steps
    }

    pub fn get_warmup_init_lr(&self) -> f64 {
        self.warmup_init_lr
    }
}

impl LearningRateScheduler for LinearLR {
    fn get_lr(&self, step: u32) -> f64 {
        if step < self.warmup_steps {
            self.warmup_init_lr
                + (self.base_lr - self.warmup_init_lr) * (step as f64 / self.warmup_steps as f64)
        } else if step < self.total_steps {
            self.base_lr
                + (self.final_lr - self.base_lr)
                    * ((step - self.warmup_steps) as f64
                        / (self.total_steps - self.warmup_steps) as f64)
        } else {
            self.final_lr
        }
    }
}

#[derive(
    AnchorSerialize,
    AnchorDeserialize,
    InitSpace,
    Serialize,
    Deserialize,
    Clone,
    Debug,
    Zeroable,
    Copy,
    PartialEq,
    TS,
)]
#[repr(C)]
pub struct CosineLR {
    base_lr: f64,
    warmup_init_lr: f64,
    final_lr: f64,
    warmup_steps: u32,
    total_steps: u32,
}

impl CosineLR {
    pub fn new(
        base_lr: f64,
        warmup_steps: u32,
        warmup_init_lr: f64,
        total_steps: u32,
        final_lr: f64,
    ) -> Self {
        CosineLR {
            base_lr,
            warmup_steps,
            warmup_init_lr,
            total_steps,
            final_lr,
        }
    }

    pub fn get_warmup_steps(&self) -> u32 {
        self.warmup_steps
    }

    pub fn get_warmup_init_lr(&self) -> f64 {
        self.warmup_init_lr
    }
}

impl LearningRateScheduler for CosineLR {
    fn get_lr(&self, step: u32) -> f64 {
        if step < self.warmup_steps {
            self.warmup_init_lr
                + (self.base_lr - self.warmup_init_lr) * (step as f64 / self.warmup_steps as f64)
        } else {
            let progress =
                (step - self.warmup_steps) as f64 / (self.total_steps - self.warmup_steps) as f64;
            let cosine_decay = 0.5 * (1.0 + (PI * progress).cos());
            self.final_lr + (self.base_lr - self.final_lr) * cosine_decay
        }
    }
}

#[derive(
    AnchorSerialize,
    AnchorDeserialize,
    InitSpace,
    Serialize,
    Deserialize,
    Clone,
    Debug,
    Zeroable,
    Copy,
    TS,
)]
#[repr(C)]
pub struct WarmupStableDecayLR {
    base_lr: f64,
    warmup_init_lr: f64,
    cosine_decay_final_lr: f64,
    linear_decay_final_lr: f64,
    warmup_steps: u32,
    stable_steps: u32,
    cosine_decay_steps: u32,
    linear_decay_steps: u32,
}

impl WarmupStableDecayLR {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        base_lr: f64,
        warmup_steps: u32,
        warmup_init_lr: f64,
        stable_steps: u32,
        cosine_decay_steps: u32,
        cosine_decay_final_lr: f64,
        linear_decay_steps: u32,
        linear_decay_final_lr: f64,
    ) -> Self {
        WarmupStableDecayLR {
            base_lr,
            warmup_steps,
            warmup_init_lr,
            stable_steps,
            cosine_decay_steps,
            cosine_decay_final_lr,
            linear_decay_steps,
            linear_decay_final_lr,
        }
    }

    pub fn get_warmup_steps(&self) -> u32 {
        self.warmup_steps
    }

    pub fn get_warmup_init_lr(&self) -> f64 {
        self.warmup_init_lr
    }
}

impl LearningRateScheduler for WarmupStableDecayLR {
    fn get_lr(&self, step: u32) -> f64 {
        assert!(self.cosine_decay_final_lr <= self.base_lr);
        assert!(self.linear_decay_final_lr <= self.cosine_decay_final_lr);
        if step < self.warmup_steps {
            self.warmup_init_lr
                + (self.base_lr - self.warmup_init_lr) * (step as f64 / self.warmup_steps as f64)
        } else if step < self.stable_steps + self.warmup_steps {
            self.base_lr
        } else if step < self.stable_steps + self.warmup_steps + self.cosine_decay_steps {
            let steps_into_decay = step - self.stable_steps - self.warmup_steps;
            let progress = steps_into_decay as f64 / self.cosine_decay_steps as f64;
            let cosine_decay = 0.5 * (1.0 + (PI * progress).cos());
            self.cosine_decay_final_lr + (self.base_lr - self.cosine_decay_final_lr) * cosine_decay
        } else if step
            < self.stable_steps
                + self.warmup_steps
                + self.cosine_decay_steps
                + self.linear_decay_steps
        {
            let steps_into_decay =
                step - self.stable_steps - self.warmup_steps - self.cosine_decay_steps;
            self.cosine_decay_final_lr
                - (self.cosine_decay_final_lr - self.linear_decay_final_lr)
                    * (steps_into_decay as f64 / self.linear_decay_steps as f64)
        } else {
            self.linear_decay_final_lr
        }
    }
}

#[derive(
    AnchorSerialize,
    AnchorDeserialize,
    InitSpace,
    Serialize,
    Deserialize,
    Clone,
    Debug,
    Zeroable,
    Copy,
    TS,
)]
#[repr(C)]
pub enum LearningRateSchedule {
    Constant(ConstantLR),
    Linear(LinearLR),
    Cosine(CosineLR),
    WarmupStableDecay(WarmupStableDecayLR),
}

impl LearningRateSchedule {
    pub fn get_lr(&self, step: u32) -> f64 {
        match self {
            Self::Constant(l) => l.get_lr(step),
            Self::Linear(l) => l.get_lr(step),
            Self::Cosine(l) => l.get_lr(step),
            Self::WarmupStableDecay(l) => l.get_lr(step),
        }
    }

    pub fn get_warmup_steps(&self) -> u32 {
        match self {
            Self::Constant(l) => l.get_warmup_steps(),
            Self::Linear(l) => l.get_warmup_steps(),
            Self::Cosine(l) => l.get_warmup_steps(),
            Self::WarmupStableDecay(l) => l.get_warmup_steps(),
        }
    }

    pub fn get_warmup_init_lr(&self) -> f64 {
        match self {
            Self::Constant(l) => l.get_warmup_init_lr(),
            Self::Linear(l) => l.get_warmup_init_lr(),
            Self::Cosine(l) => l.get_warmup_init_lr(),
            Self::WarmupStableDecay(l) => l.get_warmup_init_lr(),
        }
    }
}

impl From<CosineLR> for LearningRateSchedule {
    fn from(value: CosineLR) -> Self {
        Self::Cosine(value)
    }
}

impl From<LinearLR> for LearningRateSchedule {
    fn from(value: LinearLR) -> Self {
        Self::Linear(value)
    }
}

impl From<ConstantLR> for LearningRateSchedule {
    fn from(value: ConstantLR) -> Self {
        Self::Constant(value)
    }
}

#[derive(
    AnchorSerialize,
    AnchorDeserialize,
    InitSpace,
    Serialize,
    Deserialize,
    Clone,
    Debug,
    Zeroable,
    Copy,
    TS,
)]
#[repr(C)]
pub enum OptimizerDefinition {
    Dummy,
    AdamW {
        betas: [f32; 2],
        weight_decay: f32,
        eps: f32,
        clip_grad_norm: Option<f32>,
    },
    Distro {
        clip_grad_norm: Option<f32>,
        weight_decay: Option<f32>,
        compression_decay: f32,
        compression_topk: u16,
        compression_chunk: u16,
        quantize_1bit: bool,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn test_constant_lr() {
        let scheduler = ConstantLR::new(0.01, 10, 0.001);

        // warmup phase
        assert_relative_eq!(scheduler.get_lr(0), 0.001);
        assert_relative_eq!(scheduler.get_lr(5), 0.001 + (0.01 - 0.001) * 0.5);
        assert_relative_eq!(scheduler.get_lr(9), 0.001 + (0.01 - 0.001) * 0.9);

        // constant phase
        assert_relative_eq!(scheduler.get_lr(10), 0.01);
        assert_relative_eq!(scheduler.get_lr(100), 0.01);
        assert_relative_eq!(scheduler.get_lr(1000), 0.01);
    }

    #[test]
    fn test_linear_lr() {
        let scheduler = LinearLR::new(0.01, 10, 0.001, 100, 0.0001);

        // warmup phase
        assert_relative_eq!(scheduler.get_lr(0), 0.001);
        assert_relative_eq!(scheduler.get_lr(5), 0.001 + (0.01 - 0.001) * 0.5);

        // linear decay phase
        assert_relative_eq!(scheduler.get_lr(10), 0.01);
        assert_relative_eq!(scheduler.get_lr(55), 0.01 + (0.0001 - 0.01) * 0.5);
        assert_relative_eq!(scheduler.get_lr(100), 0.0001);

        // out of bounds (should clamp to final_lr)
        assert_relative_eq!(scheduler.get_lr(200), 0.0001);
    }

    #[test]
    fn test_cosine_lr() {
        let scheduler = CosineLR::new(0.01, 10, 0.001, 110, 0.0);

        // warmup phase
        assert_relative_eq!(scheduler.get_lr(0), 0.001);
        assert_relative_eq!(scheduler.get_lr(5), 0.001 + (0.01 - 0.001) * 0.5);

        // cosine decay phase
        assert_relative_eq!(scheduler.get_lr(10), 0.01);
        assert_relative_eq!(scheduler.get_lr(60), 0.005); // 50% of cosine cycle
        assert_relative_eq!(scheduler.get_lr(110), 0.0);
    }

    #[test]
    fn test_warmup_stable_decay_lr() {
        let scheduler = WarmupStableDecayLR::new(
            0.01, 10,    // warmup_steps
            0.001, //warmup_init_lr
            60,    // stable_steps
            110,   // cosine_decay_steps
            0.001, // cosine_decay_final_lr
            20,    // linear_decay_steps
            0.0,   // linear_decay_final_lr
        );

        // warmup phase
        assert_relative_eq!(scheduler.get_lr(0), 0.001);
        assert_relative_eq!(scheduler.get_lr(5), 0.001 + (0.01 - 0.001) * 0.5);

        // stable phase
        assert_relative_eq!(scheduler.get_lr(10), 0.01);
        assert_relative_eq!(scheduler.get_lr(30), 0.01);
        assert_relative_eq!(scheduler.get_lr(69), 0.01);

        // cosine decay phase
        assert_relative_eq!(scheduler.get_lr(70), 0.01);
        assert_relative_eq!(scheduler.get_lr(125), 0.0055, epsilon = 1e-4); // midpoint (step 125 = 70 + 110/2)
        assert_relative_eq!(scheduler.get_lr(179), 0.001, epsilon = 1e-4);

        // linear decay phase
        assert_relative_eq!(scheduler.get_lr(180), 0.001); // Start of linear decay
        assert_relative_eq!(scheduler.get_lr(190), 0.0005); // midpoint (step 190 = 180 + 20/2)
        assert_relative_eq!(scheduler.get_lr(199), 0.0001, epsilon = 1e-4);

        // final
        assert_relative_eq!(scheduler.get_lr(200), 0.0);

        // check past the end
        assert_relative_eq!(scheduler.get_lr(250), 0.0);
    }

    #[test]
    fn test_edge_cases() {
        // zero warmup steps
        let scheduler = ConstantLR::new(0.01, 0, 0.001);
        assert_relative_eq!(scheduler.get_lr(0), 0.01);

        // equal initial and base LR
        let scheduler = LinearLR::new(0.01, 10, 0.01, 100, 0.001);
        assert_relative_eq!(scheduler.get_lr(5), 0.01);

        // equal base and final LR
        let scheduler = CosineLR::new(0.01, 10, 0.001, 100, 0.01);
        assert_relative_eq!(scheduler.get_lr(50), 0.01);

        // zero-step schedule (edge case)
        let scheduler = WarmupStableDecayLR::new(0.01, 0, 0.001, 0, 0, 0.001, 0, 0.001);
        assert_relative_eq!(scheduler.get_lr(0), 0.001);
    }
}
