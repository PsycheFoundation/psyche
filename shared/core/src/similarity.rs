use anyhow::{Result, anyhow};

pub struct DistanceThresholds {
    pub jaccard_threshold: f32,
    pub manhattan_threshold: f32,
    pub hamming_threshold: f32,
}

pub fn is_similar(
    a: &[f32],
    b: &[f32],
    thresholds: &DistanceThresholds,
) -> Result<bool> {
    let manhattan = manhattan_distance(a, b)?;
    if manhattan > thresholds.manhattan_threshold {
        return Ok(false);
    }

    let hamming = hamming_distance(a, b)?;
    if hamming > thresholds.hamming_threshold {
        return Ok(false);
    }

    let jaccard = jaccard_distance(a, b);
    if jaccard > thresholds.jaccard_threshold {
        return Ok(false);
    }

    Ok(true)
}

pub fn jaccard_distance(a: &[f32], b: &[f32]) -> f32 {
    let mut intersection = 0;
    let mut union = a.len();

    for &val in b {
        if a.contains(&val) {
            intersection += 1;
        } else {
            union += 1;
        }
    }

    if union == 0 {
        return 0.0;
    }

    1.0 - (intersection as f32 / union as f32)
}

pub fn manhattan_distance(a: &[f32], b: &[f32]) -> Result<f32> {
    if a.is_empty() {
        return Err(anyhow!("Input arrays must not be empty"));
    }
    
    if a.len() != b.len() {
        return Err(anyhow!("The lengths of the arrays must be equal, but found {} and {}", a.len(), b.len()));
    }

    let mut sum = 0.0;
    for i in 0..a.len() {
        sum += (a[i] - b[i]).abs();
    }

    Ok(sum)
}

pub fn hamming_distance(a: &[f32], b: &[f32]) -> Result<f32> {
    if a.is_empty() {
        return Err(anyhow!("Input arrays must not be empty"));
    }
    
    if a.len() != b.len() {
        return Err(anyhow!("The lengths of the arrays must be equal, but found {} and {}", a.len(), b.len()));
    }

    let mut count = 0;
    for i in 0..a.len() {
        if a[i] != b[i] {
            count += 1;
        }
    }

    Ok(count as f32 / a.len() as f32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;

    fn run_jaccard_tests(a: &[f32], b: &[f32], expected: f32) {
        // Print the result
        println!("Jaccard: {}", jaccard_distance(a, b));
        assert!((jaccard_distance(a, b) - expected).abs() < 1e-6);
    }

    fn run_manhattan_tests(a: &[f32], b: &[f32], expected: Result<f32>) {
        let result = manhattan_distance(a, b);
        match (result, expected) {
            (Ok(result_val), Ok(expected_val)) => {
                assert!((result_val - expected_val).abs() < 1e-6);
            }
            (Err(err), Err(expected_err)) => {
                assert_eq!(err.to_string(), expected_err.to_string());
            }
            _ => panic!("Test failed: results did not match the expected outcome"),
        }
    }

    fn run_hamming_tests(a: &[f32], b: &[f32], expected: Result<f32>) {
        let result = hamming_distance(a, b);
        match (result, expected) {
            (Ok(result_val), Ok(expected_val)) => {
                assert!((result_val - expected_val).abs() < 1e-6);
            }
            (Err(err), Err(expected_err)) => {
                assert_eq!(err.to_string(), expected_err.to_string());
            }
            _ => panic!("Test failed: results did not match the expected outcome"),
        }
    }

    #[test]
    fn test_zero_length_inputs() {
        let a: [f32; 0] = [];
        let b: [f32; 0] = [];

        run_jaccard_tests(&a, &b, 0.0);

        run_manhattan_tests(&a, &b, Err(anyhow!("Input arrays must not be empty")));
        run_hamming_tests(&a, &b, Err(anyhow!("Input arrays must not be empty")));
    }

    #[test]
    fn test_mismatched_lengths() {
        let a = [1.0, 2.0, 3.0];
        let b = [1.0, 2.0];

        run_jaccard_tests(&a, &b, 0.333_333_34);


        run_manhattan_tests(&a, &b, Err(anyhow!("The lengths of the arrays must be equal, but found 3 and 2")));
        run_hamming_tests(&a, &b, Err(anyhow!("The lengths of the arrays must be equal, but found 3 and 2")));
    }

    #[test]
    fn test_no_intersection() {
        let a = [1.0, 2.0, 3.0];
        let b = [4.0, 5.0, 6.0];
        run_jaccard_tests(&a, &b, 1.0);
        run_manhattan_tests(&a, &b, Ok(9.0));
        run_hamming_tests(&a, &b, Ok(1.0));
    }

    #[test]
    fn test_partial_intersection() {
        let a = [1.0, 2.0, 5.0];
        let b = [1.0, 2.0, 3.0];
        run_jaccard_tests(&a, &b, 0.5);
        run_manhattan_tests(&a, &b, Ok(2.0));
        run_hamming_tests(&a, &b, Ok(0.333333));
    }

    #[test]
    fn test_complete_overlap() {
        let a = [1.0, 2.0, 3.0];
        let b = [1.0, 2.0, 3.0];
        run_jaccard_tests(&a, &b, 0.0);
        run_manhattan_tests(&a, &b, Ok(0.0));
        run_hamming_tests(&a, &b, Ok(0.0));
    }

    #[test]
    fn test_partial_match_floats() {
        let a = [0.1, 0.2, 0.3];
        let b = [0.1, 0.3, 1.0];
        run_jaccard_tests(&a, &b, 0.5);
        run_manhattan_tests(&a, &b, Ok(0.8));
        run_hamming_tests(&[0.0, 0.0, 0.0], &[0.0, 0.0, 1.0], Ok(0.333333));
    }

    #[test]
    fn test_no_match_floats() {
        let a = [0.1, 0.2, 0.3];
        let b = [0.4, 0.5, 9.1];
        run_jaccard_tests(&a, &b, 1.0);
        run_manhattan_tests(&a, &b, Ok(9.4));
        run_hamming_tests(&[0.0, 0.0, 0.0], &[0.0, 0.0, 9.0], Ok(0.333333));
    }

    #[test]
    fn test_some_intersection() {
        let a = [1.5, 2.5, 3.5, 4.5];
        let b = [2.5, 3.5, 5.5, 1.1];
        run_jaccard_tests(&a, &b, 0.6666666);
        run_manhattan_tests(&a, &b, Ok(7.4));
        run_hamming_tests(&[1.0, 2.0, 3.0, 4.0], &[2.0, 3.0, 5.0, 1.0], Ok(1.0));
    }

    #[test]
    fn test_partial_overlap_large_numbers() {
        let a = [100.0, 200.0, 300.0, 1000.0];
        let b = [100.0, 150.0, 250.0, 300.0];
        run_jaccard_tests(&a, &b, 0.6666666);
        run_manhattan_tests(&a, &b, Ok(800.0));
        run_hamming_tests(
            &[100.0, 200.0, 300.0, 1000.0],
            &[100.0, 150.0, 250.0, 300.0],
            Ok(0.75),
        );
    }

    #[test]
    fn test_is_similar_complete_match() {
        let a = [1.0, 2.0, 3.0];
        let b = [1.0, 2.0, 3.0];
        let thresholds = DistanceThresholds {
            jaccard_threshold: 0.1,
            manhattan_threshold: 1.0,
            hamming_threshold: 0.1,
        };

        assert!(is_similar(&a, &b, &thresholds).is_ok());
    }

    #[test]
    fn test_is_similar_partial_match() {
        let a = [1.0, 2.0, 3.0];
        let b = [1.0, 2.0, 5.0];
        let thresholds = DistanceThresholds {
            jaccard_threshold: 0.6,
            manhattan_threshold: 3.0,
            hamming_threshold: 0.5,
        };

        assert!(is_similar(&a, &b, &thresholds).is_ok());
    }

    #[test]
    fn test_is_similar_exceeds_manhattan() {
        let a = [1.0, 2.0, 3.0];
        let b = [1.0, 5.0, 7.0];
        let thresholds = DistanceThresholds {
            jaccard_threshold: 0.8,
            manhattan_threshold: 2.0,
            hamming_threshold: 0.8,
        };

        assert!(is_similar(&a, &b, &thresholds).is_ok_and(|x| x == false));
    }

    #[test]
    fn test_is_similar_exceeds_hamming() {
        let a = [1.0, 2.0, 3.0];
        let b = [4.0, 5.0, 6.0];
        let thresholds = DistanceThresholds {
            jaccard_threshold: 1.0,
            manhattan_threshold: 10.0,
            hamming_threshold: 0.3,
        };

        assert!(is_similar(&a, &b, &thresholds).is_ok_and(|x| x == false));
    }

    #[test]
    fn test_is_similar_exceeds_jaccard() {
        let a = [1.0, 2.0, 3.0];
        let b = [4.0, 5.0, 6.0];
        let thresholds = DistanceThresholds {
            jaccard_threshold: 0.5,
            manhattan_threshold: 10.0,
            hamming_threshold: 1.0,
        };

        assert!(is_similar(&a, &b, &thresholds).is_ok_and(|x| x == false));
    }
}
