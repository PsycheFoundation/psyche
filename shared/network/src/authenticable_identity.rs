use iroh::PublicKey;

pub fn raw_p2p_verify(signer: &[u8; 32], bytes: &[u8], signature: &[u8; 64]) -> bool {
    if let Ok(public) = PublicKey::from_bytes(signer) {
        return public
            .verify(bytes, &iroh::Signature::from_bytes(signature))
            .is_ok();
    }
    false
}
