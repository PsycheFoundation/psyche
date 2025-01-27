use crate::lcg::LCG;

/// Performs Fisher-Yates shuffle (Knuth variant) on the input slice
/// Uses provided seed for deterministic randomization
/// https://en.wikipedia.org/wiki/Fisher%E2%80%93Yates_shuffle
pub fn deterministic_shuffle<T>(items: &mut [T], seed: u64) {
    let mut rng = LCG::new(seed);

    for i in (1..items.len()).rev() {
        let j = rng.next_range(i + 1);
        items.swap(i, j);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deterministic_shuffle_same_seed() {
        let mut vec1 = vec![1, 2, 3, 4, 5];
        let mut vec2 = vec![1, 2, 3, 4, 5];

        deterministic_shuffle(&mut vec1, 42);
        deterministic_shuffle(&mut vec2, 42);

        assert_eq!(vec1, vec2);
    }

    #[test]
    fn test_deterministic_shuffle_different_seeds() {
        let mut vec1 = vec![1, 2, 3, 4, 5];
        let mut vec2 = vec![1, 2, 3, 4, 5];

        deterministic_shuffle(&mut vec1, 42);
        deterministic_shuffle(&mut vec2, 43);

        assert_ne!(vec1, vec2);
    }

    #[test]
    fn test_deterministic_shuffle_all_elements_present() {
        let mut vec = vec![1, 2, 3, 4, 5];
        let original = vec.clone();

        deterministic_shuffle(&mut vec, 42);

        assert_eq!(vec.len(), original.len());
        for &item in &original {
            assert!(vec.contains(&item));
        }
    }

    #[test]
    fn test_deterministic_shuffle_empty_vec() {
        let mut vec: Vec<i32> = Vec::new();
        deterministic_shuffle(&mut vec, 42);
        assert!(vec.is_empty());
    }

    #[test]
    fn test_deterministic_shuffle_single_element() {
        let mut vec = vec![1];
        deterministic_shuffle(&mut vec, 42);
        assert_eq!(vec, vec![1]);
    }

    #[test]
    fn test_deterministic_shuffle_large_vec() {
        let mut vec: Vec<i32> = (1..1000).collect();
        let original = vec.clone();

        deterministic_shuffle(&mut vec, 42);

        assert_ne!(vec, original);
        assert_eq!(vec.len(), original.len());
        for &item in &original {
            assert!(vec.contains(&item));
        }
    }
}
