use anchor_lang::prelude::*;

use crate::ProgramError;

#[derive(Debug, InitSpace, AnchorSerialize, AnchorDeserialize, Clone, Copy)]
pub struct Vesting {
    pub start_unix_timestamp: i64,
    pub duration_seconds: u32,
    pub end_collateral_amount: u64,
}

impl Vesting {
    pub fn compute_vested_collateral_amount(
        &self,
        now_unix_timestamp: i64,
    ) -> Result<i128> {
        let elapsed_seconds = now_unix_timestamp
            .checked_sub(self.start_unix_timestamp)
            .ok_or(ProgramError::MathOverflow)?;
        if elapsed_seconds < 0 {
            return Ok(0);
        }
        if elapsed_seconds >= i64::from(self.duration_seconds) {
            return Ok(i128::from(self.end_collateral_amount));
        }
        Ok(i128::from(self.end_collateral_amount)
            .checked_mul(i128::from(elapsed_seconds))
            .ok_or(ProgramError::MathOverflow)?
            .checked_div(i128::from(self.duration_seconds))
            .ok_or(ProgramError::MathOverflow)?)
    }
}
