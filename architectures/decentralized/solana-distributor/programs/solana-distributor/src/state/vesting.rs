use anchor_lang::prelude::*;

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
    ) -> u64 {
        if now_unix_timestamp < self.start_unix_timestamp {
            return 0;
        }
        if self.duration_seconds == 0 {
            return self.end_collateral_amount;
        }

        let elapsed_seconds =
            u128::try_from(now_unix_timestamp - self.start_unix_timestamp)
                .unwrap();
        let duration_seconds = u128::from(self.duration_seconds);
        let end_collateral_amount = u128::from(self.end_collateral_amount);

        let vested_collateral_amount =
            end_collateral_amount * elapsed_seconds / duration_seconds;
        if vested_collateral_amount > end_collateral_amount {
            return self.end_collateral_amount;
        }

        u64::try_from(vested_collateral_amount).unwrap()
    }
}
