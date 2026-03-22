use anchor_lang::solana_program::hash::{hash, hashv};

pub fn sha256(data: &[u8]) -> [u8; 32] {
    let hash_result = hash(data);
    hash_result.to_bytes()
}

pub fn sha256v(data: &[&[u8]]) -> [u8; 32] {
    let hash_result = hashv(data);
    hash_result.to_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sha256() {
        let data = b"Hello, world!";
        let hash = sha256(data);
        assert_eq!(
            hash,
            [
                0x31, 0x5f, 0x5b, 0xdb, 0x76, 0xd0, 0x78, 0xc4, 0x3b, 0x8a, 0xc0, 0x06, 0x4e, 0x4a,
                0x01, 0x64, 0x61, 0x2b, 0x1f, 0xce, 0x77, 0xc8, 0x69, 0x34, 0x5b, 0xfc, 0x94, 0xc7,
                0x58, 0x94, 0xed, 0xd3
            ]
        );
    }
}
