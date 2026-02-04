use anchor_lang::{AnchorDeserialize, AnchorSerialize};
use bytemuck::Zeroable;
use psyche_coordinator::{Coordinator, HealthChecks, model};
use psyche_core::NodeIdentity;
use psyche_network::{
    AuthenticatableIdentity, EndpointId, FromSignedBytesError, PublicKey, SecretKey, SignedMessage,
};
use psyche_watcher::OpportunisticData;
use serde::{Deserialize, Serialize};
use std::fmt::Display;
use ts_rs::TS;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ClientToServerMessage {
    Join { run_id: String },
    Witness(Box<OpportunisticData>),
    HealthCheck(HealthChecks<ClientId>),
    Checkpoint(model::HubRepo),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ServerToClientMessage {
    Coordinator(Box<Coordinator<ClientId>>),
}

#[derive(Serialize, Deserialize, Clone, Hash, PartialEq, Eq, Debug, Copy, TS)]
#[ts(type = "string")]
pub struct ClientId(EndpointId);

impl Default for ClientId {
    fn default() -> Self {
        Self(PublicKey::from_bytes(&[0u8; 32]).unwrap())
    }
}

impl Display for ClientId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{}", self.0.fmt_short()))?;
        Ok(())
    }
}

impl NodeIdentity for ClientId {
    fn get_p2p_public_key(&self) -> &[u8; 32] {
        self.0.as_bytes()
    }
}

unsafe impl Zeroable for ClientId {
    fn zeroed() -> Self {
        unsafe { core::mem::zeroed() }
    }
}

impl AuthenticatableIdentity for ClientId {
    type PrivateKey = SecretKey;
    fn from_signed_challenge_bytes(
        bytes: &[u8],
        challenge: [u8; 32],
    ) -> Result<Self, FromSignedBytesError> {
        let (key, decoded_challenge) = SignedMessage::<[u8; 32]>::verify_and_decode(bytes)
            .map_err(|_| FromSignedBytesError::Deserialize)?;
        if decoded_challenge != challenge {
            return Err(FromSignedBytesError::MismatchedChallenge(
                challenge,
                decoded_challenge.into(),
            ));
        }
        Ok(Self(key))
    }

    fn to_signed_challenge_bytes(
        &self,
        private_key: &Self::PrivateKey,
        challenge: [u8; 32],
    ) -> Vec<u8> {
        assert_eq!(private_key.public(), self.0);
        SignedMessage::sign_and_encode(private_key, &challenge)
            .expect("alloc error")
            .to_vec()
    }

    fn get_p2p_public_key(&self) -> &[u8; 32] {
        self.0.as_bytes()
    }

    fn raw_p2p_sign(&self, private_key: &Self::PrivateKey, bytes: &[u8]) -> [u8; 64] {
        private_key.sign(bytes).to_bytes()
    }
}

impl From<PublicKey> for ClientId {
    fn from(value: PublicKey) -> Self {
        Self(value)
    }
}

impl AsRef<[u8]> for ClientId {
    fn as_ref(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

impl AnchorSerialize for ClientId {
    fn serialize<W: std::io::Write>(&self, _: &mut W) -> std::io::Result<()> {
        unimplemented!()
    }
}

impl AnchorDeserialize for ClientId {
    fn deserialize_reader<R: std::io::Read>(_: &mut R) -> std::io::Result<Self> {
        unimplemented!()
    }
}

impl anchor_lang::Space for ClientId {
    const INIT_SPACE: usize = 0;
}
