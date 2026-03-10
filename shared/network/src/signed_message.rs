use crate::Networkable;

use anyhow::Result;
use bytes::Bytes;
use iroh::{PublicKey, SecretKey};
use serde::{Deserialize, Serialize};
use std::marker::PhantomData;

#[derive(Debug, Serialize, Deserialize)]
pub struct SignedMessage<T: Networkable> {
    from: PublicKey,
    data: Bytes,
    signature: iroh::Signature,
    _t: PhantomData<T>,
}

impl<T: Networkable> SignedMessage<T> {
    pub fn verify_and_decode(bytes: &[u8]) -> Result<(PublicKey, T)> {
        let signed_message: Self = postcard::from_bytes(bytes)?;
        let key: PublicKey = signed_message.from;
        key.verify(&signed_message.data, &signed_message.signature)?;
        let message: T = postcard::from_bytes(&signed_message.data)?;
        Ok((signed_message.from, message))
    }

    pub fn sign_and_encode(secret_key: &SecretKey, message: &T) -> Result<Bytes> {
        let data: Bytes = postcard::to_stdvec(&message)?.into();
        let signature = secret_key.sign(&data);
        let from: PublicKey = secret_key.public();
        let signed_message = Self {
            from,
            data,
            signature,
            _t: Default::default(),
        };
        let encoded = postcard::to_stdvec(&signed_message)?;
        Ok(encoded.into())
    }
}
