use crate::sha::sha256v;

const SHUFFLE_ROUND_COUNT: u8 = 90;

pub fn compute_shuffled_index(index: u64, index_count: u64, seed: &[u8; 32]) -> u64 {
    assert!(index < index_count);

    let mut current_index = index;

    for current_round in 0..SHUFFLE_ROUND_COUNT {
        let hash_result = sha256v(&[seed, &[current_round]]);

        let pivot = u64::from_le_bytes(hash_result[0..8].try_into().unwrap()) % index_count;
        let flip = (pivot + index_count - current_index) % index_count;
        let position = current_index.max(flip);

        let source = sha256v(&[
            seed,
            &[current_round],
            &(position / 256).to_le_bytes()[0..4],
        ]);

        let byte = source[(position % 256) as usize / 8];
        let bit = (byte >> (position % 8)) % 2;

        current_index = if bit == 1 { flip } else { current_index };
    }

    current_index
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_shuffled_index_basic() {
        let seed = [0u8; 32];
        let index_count = 10;

        for i in 0..index_count {
            let result = compute_shuffled_index(i, index_count, &seed);
            assert!(
                result < index_count,
                "Shuffled index should be within bounds"
            );
        }
    }

    #[test]
    fn test_compute_shuffled_index_deterministic() {
        let seed = [1u8; 32];
        let index_count = 100;

        for i in 0..index_count {
            let result1 = compute_shuffled_index(i, index_count, &seed);
            let result2 = compute_shuffled_index(i, index_count, &seed);
            assert_eq!(
                result1, result2,
                "Results should be deterministic for the same input"
            );
        }
    }

    #[test]
    fn test_compute_shuffled_index_different_seeds() {
        let seed1 = [1u8; 32];
        let seed2 = [2u8; 32];
        let index = 5;
        let index_count = 100;

        let result1 = compute_shuffled_index(index, index_count, &seed1);
        let result2 = compute_shuffled_index(index, index_count, &seed2);
        assert_ne!(
            result1, result2,
            "Different seeds should produce different results"
        );
    }

    #[test]
    #[should_panic(expected = "index < index_count")]
    fn test_compute_shuffled_index_out_of_bounds() {
        let seed = [0u8; 32];
        let index_count = 10;
        compute_shuffled_index(index_count, index_count, &seed);
    }
}
