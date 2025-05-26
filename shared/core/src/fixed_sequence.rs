pub const BATCH_SIZE_INDEX_BITS: u8 = 6;

/// Finds the index that corresponds to the value. Index 0 is for value 0.
/// `index_bits` determines the number of distinct non-zero indices, which is (2^index_bits - 1).
/// E.g., index_bits = 6 means 63 non-zero indices (1 to 63).
pub fn value_to_nearest_index(value: u16, max_data_value: u16, index_bits: u8) -> u8 {
    if value == 0 || max_data_value == 0 || index_bits == 0 {
        return 0;
    }

    // Cap index_bits at 8, as the return type is u8 (max index 255).
    let effective_index_bits = index_bits.min(8);
    if effective_index_bits == 0 {
        // Should be caught by index_bits == 0, but defensive.
        return 0;
    }

    // Number of distinct non-zero indices we can represent.
    // e.g., if index_bits = 1, num_non_zero_indices = 1 (index 1)
    // e.g., if index_bits = 6, num_non_zero_indices = 63 (indices 1-63)
    let num_non_zero_indices = (1u32 << effective_index_bits).saturating_sub(1) as u16;

    if num_non_zero_indices == 0 {
        // Should not happen if effective_index_bits >= 1
        return 0;
    }

    // Case 1: Direct Mapping
    // If max_data_value can be directly represented by the available indices.
    if max_data_value <= num_non_zero_indices {
        // Clamp value to the valid range [1, max_data_value].
        // The index is the value itself. max_data_value <= 255 here, so u8 cast is safe.
        return value.clamp(1, max_data_value) as u8;
    }

    // Case 2: Scaling
    // max_data_value is larger than num_non_zero_indices.
    // Map the range [1, max_data_value] to indices [1, num_non_zero_indices].
    let clamped_target_value = value.clamp(1, max_data_value);

    if num_non_zero_indices == 1 {
        // Only one non-zero index available (index 1). All non-zero values map to it.
        return 1;
    }

    // num_non_zero_indices > 1.
    // Map [1, max_data_value] to zero-based levels [0, num_non_zero_indices - 1], then add 1 for 1-based index.
    // Denominator (num_non_zero_indices - 1) is >= 1.
    // Numerator (max_data_value - 1) is > (num_non_zero_indices - 1) because max_data_value > num_non_zero_indices.
    // So increment will be > 0 (unless max_data_value -1 == 0, but max_data_value > num_non_zero_indices >=2 implies max_data_value >=3).
    let increment = (max_data_value - 1) as f64 / (num_non_zero_indices - 1) as f64;

    let zero_based_level_float = ((clamped_target_value - 1) as f64) / increment;
    let mut zero_based_level_rounded = zero_based_level_float.round() as i64;
    zero_based_level_rounded = zero_based_level_rounded.clamp(0, (num_non_zero_indices - 1) as i64);

    (zero_based_level_rounded as u8) + 1
}

/// Calculates the actual value from an index, max_data_value, and index_bits.
/// `index_bits` determines the number of distinct non-zero indices: (2^index_bits - 1).
pub fn index_to_value(idx: u8, max_data_value: u16, index_bits: u8) -> u16 {
    if idx == 0 {
        return 0;
    }
    // If idx > 0, then max_data_value and index_bits should also be > 0
    // as per the logic in value_to_nearest_index.
    if max_data_value == 0 || index_bits == 0 {
        return 0; // Should ideally not be reached if idx > 0 from valid conversion
    }

    let effective_index_bits = index_bits.min(8);
    if effective_index_bits == 0 {
        return 0;
    }

    let num_non_zero_indices = (1u32 << effective_index_bits).saturating_sub(1) as u16;

    if num_non_zero_indices == 0 {
        // Should not happen
        return 0;
    }

    // Assuming 1 <= idx <= num_non_zero_indices, as value_to_nearest_index should ensure this.

    // Case 1: Direct Mapping
    if max_data_value <= num_non_zero_indices {
        // The value is the index itself.
        return idx as u16;
    }

    // Case 2: Scaling
    // max_data_value is larger than num_non_zero_indices.
    // Map index [1, num_non_zero_indices] back to value in [1, max_data_value].

    if num_non_zero_indices == 1 {
        // If there's only one non-zero index (idx must be 1), it maps to max_data_value (or 1 if max_data_value is 1).
        return max_data_value.max(1);
    }

    // num_non_zero_indices > 1.
    // `idx` is 1-based. Convert to 0-based for formula.
    let zero_based_idx_for_formula = idx - 1; // idx is u8

    let increment = if max_data_value == 1 {
        0.0 // All values are 1.
    } else {
        // Denominator (num_non_zero_indices - 1) is >= 1.
        (max_data_value - 1) as f64 / (num_non_zero_indices - 1) as f64
    };

    let value_float = 1.0 + (zero_based_idx_for_formula as f64 * increment);

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
