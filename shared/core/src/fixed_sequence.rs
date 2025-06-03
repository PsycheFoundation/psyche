pub const BATCH_SIZE_INDEX_BITS: u8 = 8;

// In this file, we define functions to convert between values and indices
// in a mapping from [1, max_data_value] to [1, 2^index_bits - 1].
// This is useful for representing values in a compact form when we are limited by the number of bits available for indexing.

/// Converts a value to the nearest index in a mapping from [1, max_data_value] to [1, 2^index_bits - 1].
pub fn value_to_nearest_index(value: u16, max_data_value: u16, index_bits: u8) -> u8 {
    assert!(index_bits <= 8, "index_bits must be 8 or less");
    assert!(index_bits > 1, "index_bits must be greater than 1");

    if value == 0 || max_data_value == 0 {
        return 0;
    }

    // Number of distinct non-zero indices we can represent.
    // For index_bits = 6 it's 63 (1 to 63)
    let num_non_zero_indices = (1u32 << index_bits).saturating_sub(1) as u16;

    // In this case we can map 1-1 since we have enough bits
    if max_data_value <= num_non_zero_indices {
        // Clamp value to the valid range [1, max_data_value].
        return value.clamp(1, max_data_value) as u8;
    }

    // We don't have enough bits so we map the range [1, max_data_value] to indices [1, num_non_zero_indices].
    let clamped_target_value = value.clamp(1, max_data_value);

    // Special case where if we have only one non-zero index then it must be 1
    if num_non_zero_indices == 1 {
        return 1;
    }

    // Map [1, max_data_value] to zero-based levels [0, num_non_zero_indices - 1], then add 1 for 1-based index.
    // Denominator (num_non_zero_indices - 1) is >= 1.
    // Numerator (max_data_value - 1) is > (num_non_zero_indices - 1) because max_data_value > num_non_zero_indices.
    // So increment will be > 0
    let increment = (max_data_value - 1) as f64 / (num_non_zero_indices - 1) as f64;

    let zero_based_level_float = ((clamped_target_value - 1) as f64) / increment;
    let mut zero_based_level_rounded = zero_based_level_float.round() as i64;
    zero_based_level_rounded = zero_based_level_rounded.clamp(0, (num_non_zero_indices - 1) as i64);

    (zero_based_level_rounded as u8) + 1
}

/// Converts an index back to the corresponding value in the mapping from [1, max_data_value] to [1, 2^index_bits - 1].
pub fn index_to_value(idx: u8, max_data_value: u16, index_bits: u8) -> u16 {
    assert!(index_bits <= 8, "index_bits must be 8 or less");
    assert!(index_bits > 1, "index_bits must be greater than 1");

    if idx == 0 || max_data_value == 0 {
        return 0;
    }

    let num_non_zero_indices = (1u32 << index_bits).saturating_sub(1) as u16;

    // Assuming 1 <= idx <= num_non_zero_indices:

    // In this case we have enough bits to map 1-1 so the value must be the index itself
    if max_data_value <= num_non_zero_indices {
        return idx as u16;
    }

    // Special case where if we have only one non-zero index then it must be the max number
    // Since we would have [0, 1] -> [0, max_data_value]
    if num_non_zero_indices == 1 {
        return max_data_value.max(1);
    }

    // num_non_zero_indices > 1:
    let increment = (max_data_value - 1) as f64 / (num_non_zero_indices - 1) as f64;
    let value_float = 1.0 + ((idx - 1) as f64 * increment); // `idx` is 1-based. Convert to 0-based for formula.

    (value_float.round().clamp(1.0, max_data_value as f64)) as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_direct_mapping_small_max_value() {
        let max_data_value = 20;
        let index_bits = 6; // 63 non-zero indices

        assert_eq!(value_to_nearest_index(0, max_data_value, index_bits), 0);
        assert_eq!(index_to_value(0, max_data_value, index_bits), 0);

        for i in 1..=max_data_value {
            let index = value_to_nearest_index(i, max_data_value, index_bits);
            assert_eq!(index, i as u8, "Value {} -> Index", i);
            let calculated_value = index_to_value(index, max_data_value, index_bits);
            assert_eq!(calculated_value, i, "Index {} -> Value", index);
        }
        // Test clamping for values > max_data_value
        assert_eq!(
            value_to_nearest_index(max_data_value + 5, max_data_value, index_bits),
            max_data_value as u8
        );
    }

    #[test]
    fn test_scaling_large_max_value_6bits() {
        let max_data_value = 1000;
        let index_bits = 6; // 63 non-zero indices

        assert_eq!(value_to_nearest_index(0, max_data_value, index_bits), 0);
        assert_eq!(index_to_value(0, max_data_value, index_bits), 0);

        // Check endpoints
        assert_eq!(value_to_nearest_index(1, max_data_value, index_bits), 1);
        assert_eq!(index_to_value(1, max_data_value, index_bits), 1);

        assert_eq!(
            value_to_nearest_index(max_data_value, max_data_value, index_bits),
            63
        );
        assert_eq!(
            index_to_value(63, max_data_value, index_bits),
            max_data_value
        );

        // Check an intermediate value
        // Value 500 should map to an index. (1000-1)/(63-1) = 999/62 approx 16.11
        // (500-1)/16.11 = 499/16.11 = 30.97 -> round to 31. Index = 31+1 = 32.
        let val_in = 500;
        let idx_out = value_to_nearest_index(val_in, max_data_value, index_bits);
        assert_eq!(idx_out, 32);
        // index_to_value(32, 1000, 6) -> 1 + (31 * 16.1129) = 1 + 499.5 = 500.5 -> round 501
        assert_eq!(index_to_value(idx_out, max_data_value, index_bits), 501); // Example of precision loss

        // Test round trip for some values (expect approximation)
        let values_to_test = [1, 10, 100, 500, 900, 999, 1000];
        for v_orig in values_to_test {
            let idx = value_to_nearest_index(v_orig, max_data_value, index_bits);
            let v_reconstructed = index_to_value(idx, max_data_value, index_bits);
            println!(
                "Orig: {}, Index: {}, Recon: {}",
                v_orig, idx, v_reconstructed
            );
            // Check if reconstructed is close, e.g. within ~ increment/2
            let increment = (max_data_value - 1) as f64 / (((1 << index_bits) - 1) - 1) as f64; // approx 16
            assert!(
                (v_reconstructed as i32 - v_orig as i32).abs()
                    <= (increment / 2.0).round() as i32 + 1, // +1 for rounding variance
                "Value {} -> Index {} -> Value {} (too far)",
                v_orig,
                idx,
                v_reconstructed
            );
        }
    }

    #[test]
    fn test_max_bits_8bits_scaling() {
        let max_data_value = 5000;
        let index_bits = 8; // 255 non-zero indices

        assert_eq!(value_to_nearest_index(0, max_data_value, index_bits), 0);
        assert_eq!(index_to_value(0, max_data_value, index_bits), 0);

        assert_eq!(value_to_nearest_index(1, max_data_value, index_bits), 1);
        assert_eq!(index_to_value(1, max_data_value, index_bits), 1);

        assert_eq!(
            value_to_nearest_index(max_data_value, max_data_value, index_bits),
            255
        );
        assert_eq!(
            index_to_value(255, max_data_value, index_bits),
            max_data_value
        );
    }
}
