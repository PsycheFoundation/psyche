use anchor_client::solana_sdk::{
    signature::{Keypair, SIGNATURE_BYTES, Signature},
    signer::Signer,
};
use anyhow::Result;
use psyche_core::NodeIdentity;
use psyche_network::{AuthenticatableIdentity, FromSignedBytesError, SecretKey, SignedMessage};
use std::sync::Arc;

#[derive(Clone, Debug, Copy)]
pub struct NetworkIdentity(psyche_solana_coordinator::ClientId);

impl AsRef<[u8]> for NetworkIdentity {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl std::hash::Hash for NetworkIdentity {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

impl std::cmp::PartialEq for NetworkIdentity {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl std::cmp::Eq for NetworkIdentity {}

impl std::fmt::Display for NetworkIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AuthenticatableIdentity for NetworkIdentity {
    type PrivateKey = (Arc<Keypair>, SecretKey);

    fn from_signed_challenge_bytes(
        bytes: &[u8],
        challenge: [u8; 32],
    ) -> Result<Self, FromSignedBytesError> {
        let (p2p_identity, decoded_challenge) = SignedMessage::<Vec<u8>>::verify_and_decode(bytes)
            .map_err(|_| FromSignedBytesError::Deserialize)?;
        if decoded_challenge.len() != SIGNATURE_BYTES + 32 {
            return Err(FromSignedBytesError::Deserialize);
        }
        let (signature, pubkey) = decoded_challenge.split_at(SIGNATURE_BYTES);
        let signature: Signature = signature.try_into().unwrap();
        if !signature.verify(pubkey, &challenge) {
            return Err(FromSignedBytesError::MismatchedChallenge(
                challenge,
                decoded_challenge,
            ));
        }
        let mut owner: [u8; 32] = [0; 32];
        owner.copy_from_slice(pubkey);
        Ok(Self(psyche_solana_coordinator::ClientId {
            signer: owner.into(),
            p2p_identity: *p2p_identity.as_bytes(),
        }))
    }

    fn to_signed_challenge_bytes(
        &self,
        private_key: &Self::PrivateKey,
        challenge: [u8; 32],
    ) -> Vec<u8> {
        assert_eq!(private_key.0.pubkey(), self.0.signer);
        assert_eq!(private_key.1.public().as_bytes(), &self.0.p2p_identity);
        let challenge = private_key.0.sign_message(&challenge);
        SignedMessage::<Vec<u8>>::sign_and_encode(
            &private_key.1,
            &[challenge.as_ref(), &self.0.signer.to_bytes()].concat(),
        )
        .expect("alloc error")
        .to_vec()
    }

    fn get_p2p_public_key(&self) -> &[u8; 32] {
        self.0.get_p2p_public_key()
    }

    fn raw_p2p_sign(&self, private_key: &Self::PrivateKey, bytes: &[u8]) -> [u8; 64] {
        assert_eq!(private_key.0.pubkey(), self.0.signer);
        assert_eq!(private_key.1.public().as_bytes(), &self.0.p2p_identity);
        let signature = private_key.1.sign(bytes);
        signature.to_bytes()
    }
}

impl From<psyche_solana_coordinator::ClientId> for NetworkIdentity {
    fn from(value: psyche_solana_coordinator::ClientId) -> Self {
        Self(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::Rng;

    fn generate_random_challenge() -> [u8; 32] {
        let mut rng = rand::thread_rng();
        let mut challenge = [0u8; 32];
        rng.fill(&mut challenge);
        challenge
    }

    #[test]
    fn test_network_identity_roundtrip() {
        let keypair = Arc::new(Keypair::new());
        let secret_key = SecretKey::generate(&mut rand::rngs::OsRng);
        let private_key = (keypair.clone(), secret_key.clone());

        let client_id = psyche_solana_coordinator::ClientId {
            signer: keypair.pubkey(),
            p2p_identity: *secret_key.public().as_bytes(),
        };

        let network_identity = NetworkIdentity(client_id);

        let challenge = generate_random_challenge();

        let signed_bytes = network_identity.to_signed_challenge_bytes(&private_key, challenge);
        let decoded_identity =
            NetworkIdentity::from_signed_challenge_bytes(&signed_bytes, challenge)
                .expect("Failed to decode signed bytes");

        assert_eq!(network_identity, decoded_identity);
        assert_eq!(network_identity.0.signer, decoded_identity.0.signer);
        assert_eq!(
            network_identity.0.p2p_identity,
            decoded_identity.0.p2p_identity
        );
    }

    #[test]
    fn test_network_identity_invalid_challenge() {
        let keypair = Arc::new(Keypair::new());
        let secret_key = SecretKey::generate(&mut rand::rngs::OsRng);
        let private_key = (keypair.clone(), secret_key.clone());

        let client_id = psyche_solana_coordinator::ClientId {
            signer: keypair.pubkey(),
            p2p_identity: *secret_key.public().as_bytes(),
        };

        let network_identity = NetworkIdentity(client_id);
        let challenge1 = generate_random_challenge();
        let challenge2 = generate_random_challenge();

        // sign with challenge1 but verify with challenge2
        let signed_bytes = network_identity.to_signed_challenge_bytes(&private_key, challenge1);
        let result = NetworkIdentity::from_signed_challenge_bytes(&signed_bytes, challenge2);

        assert!(result.is_err());
        match result {
            Err(FromSignedBytesError::MismatchedChallenge(_, _)) => (),
            _ => panic!("Expected MismatchedChallenge error"),
        }
    }

    #[test]
    fn test_network_identity_display() {
        let keypair = Arc::new(Keypair::new());
        let secret_key = SecretKey::generate(&mut rand::rngs::OsRng);
        let client_id = psyche_solana_coordinator::ClientId {
            signer: keypair.pubkey(),
            p2p_identity: *secret_key.public().as_bytes(),
        };
        let network_identity = NetworkIdentity(client_id);

        let display_string = format!("{network_identity}");
        assert_eq!(display_string, format!("{client_id}"));
    }

    #[test]
    fn test_network_identity_hash() {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let keypair = Arc::new(Keypair::new());
        let secret_key = SecretKey::generate(&mut rand::rngs::OsRng);
        let client_id = psyche_solana_coordinator::ClientId {
            signer: keypair.pubkey(),
            p2p_identity: *secret_key.public().as_bytes(),
        };
        let network_identity = NetworkIdentity(client_id);

        let mut hasher1 = DefaultHasher::new();
        let mut hasher2 = DefaultHasher::new();

        network_identity.hash(&mut hasher1);
        client_id.hash(&mut hasher2);

        assert_eq!(hasher1.finish(), hasher2.finish());
    }
}
