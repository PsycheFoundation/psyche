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
    ) -> Result<u64> {
        if now_unix_timestamp < self.start_unix_timestamp {
            return Ok(0);
        }
        if self.duration_seconds == 0 {
            return Ok(self.end_collateral_amount);
        }

        let elapsed_seconds =
            u128::try_from(now_unix_timestamp - self.start_unix_timestamp)
                .map_err(|_| ProgramError::VestingElapsedSecondsOverflow)?;

        let duration_seconds = u128::from(self.duration_seconds);
        let end_collateral_amount = u128::from(self.end_collateral_amount);

        let vested_collateral_amount = end_collateral_amount
            .checked_mul(elapsed_seconds)
            .ok_or(ProgramError::VestingNumeratorCollateralAmountOverflow)?
            .checked_div(duration_seconds)
            .ok_or(ProgramError::VestingDenominatorCollateralAmountOverflow)?;

        if vested_collateral_amount > end_collateral_amount {
            return Ok(self.end_collateral_amount);
        }

        Ok(u64::try_from(vested_collateral_amount)
            .map_err(|_| ProgramError::VestingVestedCollateralAmountOverflow)?)
    }
}
