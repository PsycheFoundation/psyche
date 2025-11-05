use anchor_lang::prelude::*;
use bytemuck::Pod;
use bytemuck::Zeroable;
use psyche_core::FixedVec;
use psyche_core::SizedIterator;
use serde::Deserialize;
use serde::Serialize;
use ts_rs::TS;

use crate::client::Client;
use crate::program_error::ProgramError;
use crate::ClientId;
use crate::SOLANA_MAX_NUM_PENDING_CLIENTS;

#[derive(
    Debug,
    Clone,
    Copy,
    Zeroable,
    AnchorSerialize,
    AnchorDeserialize,
    Serialize,
    Deserialize,
    TS,
)]
#[repr(C)]
pub struct ClientsState {
    pub clients: FixedVec<Client, { SOLANA_MAX_NUM_PENDING_CLIENTS }>,
    pub next_active: u64,
    pub current_epoch_rates: ClientsEpochRates,
    pub future_epoch_rates: ClientsEpochRates,
}

#[derive(
    Debug,
    Clone,
    Copy,
    Zeroable,
    AnchorSerialize,
    AnchorDeserialize,
    Serialize,
    Deserialize,
    TS,
)]
#[repr(C)]
pub struct ClientsEpochRates {
    pub earning_rate_total_shared: u64,
    pub slashing_rate_per_client: u64,
}

unsafe impl Pod for ClientsState {}

impl ClientsState {
    pub fn get_active_clients_ids(
        &self,
    ) -> SizedIterator<impl Iterator<Item = &ClientId>> {
        let mut size = 0;
        for x in self.clients.iter() {
            if x.active == self.next_active {
                size += 1;
            }
        }
        let iter = self.clients.iter().filter_map(move |x| {
            match x.active == self.next_active {
                true => Some(&x.id),
                false => None,
            }
        });
        SizedIterator::new(iter, size)
    }

    pub fn find_signer(&self, signer: &Pubkey) -> Result<&ClientId> {
        match self.clients.iter().find(|x| x.id.signer == *signer) {
            Some(client) => Ok(&client.id),
            None => err!(ProgramError::SignerNotAClient),
        }
    }
}
