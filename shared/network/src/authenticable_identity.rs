use iroh::PublicKey;
use std::{
    fmt::{Debug, Display},
    hash::Hash,
};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum FromSignedBytesError {
    #[error("bytes are not a valid AuthenticatableIdentity.")]
    Deserialize,

    #[error("challenge doesn't match expected challenge: {0:?} != {1:?}")]
    MismatchedChallenge([u8; 32], Vec<u8>),
}

pub trait AuthenticatableIdentity:
    Send + Sync + Clone + Display + Sized + Hash + Eq + Debug
{
    type PrivateKey: Send + Sync + Clone;
    fn from_signed_challenge_bytes(
        bytes: &[u8],
        challenge: [u8; 32],
    ) -> Result<Self, FromSignedBytesError>;
    fn to_signed_challenge_bytes(
        &self,
        private_key: &Self::PrivateKey,
        challenge: [u8; 32],
    ) -> Vec<u8>;
    fn get_p2p_public_key(&self) -> &[u8; 32];
    fn raw_p2p_sign(&self, private_key: &Self::PrivateKey, bytes: &[u8]) -> [u8; 64];
}

pub fn raw_p2p_verify(signer: &[u8; 32], bytes: &[u8], signature: &[u8; 64]) -> bool {
    if let Ok(public) = PublicKey::from_bytes(signer) {
        return public
            .verify(bytes, &iroh::Signature::from_bytes(signature))
            .is_ok();
    }
    false
}
