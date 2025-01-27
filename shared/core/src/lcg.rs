/// Linear congruential generator implementation
/// See: https://en.wikipedia.org/wiki/Linear_congruential_generator
const LCG_A: u64 = 6364136223846793005;
const LCG_C: u64 = 1442695040888963407;

/// Provides deterministic random number generation using the LCG algorithm
pub struct LCG {
    state: u64,
}

impl LCG {
    /// Creates a new LCG with the given seed value
    pub fn new(seed: u64) -> Self {
        LCG { state: seed }
    }

    /// Generates the next random u64 value
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_mul(LCG_A).wrapping_add(LCG_C);
        self.state
    }

    /// Generates a random number in range [0, max)
    pub fn next_range(&mut self, max: usize) -> usize {
        (self.next_u64() % max as u64) as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lcg_initialization() {
        let lcg = LCG::new(12345);
        assert_eq!(lcg.state, 12345);
    }

    #[test]
    fn test_lcg_next() {
        let mut lcg = LCG::new(12345);
        let first = lcg.next_u64();
        let second = lcg.next_u64();
        assert_ne!(first, second);
    }

    #[test]
    fn test_lcg_sequence() {
        let mut lcg = LCG::new(12345);
        let sequence: Vec<u64> = (0..5).map(|_| lcg.next_u64()).collect();
        assert_eq!(sequence.len(), 5);
        assert!(sequence.windows(2).all(|w| w[0] != w[1]));
    }

    #[test]
    fn test_lcg_reproducibility() {
        let mut lcg1 = LCG::new(12345);
        let mut lcg2 = LCG::new(12345);
        for _ in 0..100 {
            assert_eq!(lcg1.next_u64(), lcg2.next_u64());
        }
    }

    #[test]
    fn test_lcg_next_range() {
        let mut lcg = LCG::new(12345);
        let max = 10;
        for _ in 0..1000 {
            let value = lcg.next_range(max);
            assert!(value < max);
        }
    }

    #[test]
    fn test_lcg_next_range_distribution() {
        let mut lcg = LCG::new(12345);
        let max = 10;
        let mut counts = vec![0; max];
        for _ in 0..10000 {
            let value = lcg.next_range(max);
            counts[value] += 1;
        }
        // Check that each number appears at least once
        assert!(counts.iter().all(|&count| count > 0));
    }

    #[test]
    fn test_lcg_next_range_edge_cases() {
        let mut lcg = LCG::new(12345);
        assert_eq!(lcg.next_range(1), 0);

        let large_max = usize::MAX;
        let large_value = lcg.next_range(large_max);
        assert!(large_value < large_max);
    }
}
