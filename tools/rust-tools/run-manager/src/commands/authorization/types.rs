use anchor_client::solana_sdk::pubkey::Pubkey;
use anchor_client::solana_sdk::system_program;
use std::str::FromStr;

#[derive(Debug, Clone)]
pub enum Authorization {
    Address(Pubkey),
    Permissionless,
}

impl Authorization {
    /// Convert to Pubkey. Permissionless maps to system_program::ID (11111111...)
    pub fn to_pubkey(&self) -> Pubkey {
        match self {
            Authorization::Address(pubkey) => *pubkey,
            Authorization::Permissionless => system_program::ID,
        }
    }
}

impl FromStr for Authorization {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.eq_ignore_ascii_case("permissionless") {
            Ok(Authorization::Permissionless)
        } else {
            let pubkey = Pubkey::from_str(s)
                .map_err(|e| anyhow::anyhow!("Invalid pubkey '{}': {}", s, e))?;
            Ok(Authorization::Address(pubkey))
        }
    }
}
