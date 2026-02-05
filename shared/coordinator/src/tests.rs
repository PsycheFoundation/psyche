use super::*;

#[test]
fn test_new_committee_selection() {
    let cs = CommitteeSelection::new(10, 20, 30, 100, 12345).unwrap();
    assert_eq!(cs.tie_breaker_nodes, 10);
    assert_eq!(cs.witness_nodes, 20);
    assert_eq!(cs.verifier_nodes, 27); // (100 - 10) * 30% = 27
    assert_eq!(cs.total_nodes, 100);
}

#[test]
fn test_get_committee() {
    let cs = CommitteeSelection::new(10, 20, 30, 100, 12345).unwrap();

    // Test for all possible indexes
    for i in 0..100 {
        let proof = cs.get_committee(i);
        assert!(proof.position < 100);

        // Verify that the committee matches the position
        match proof.committee {
            Committee::TieBreaker => assert!(proof.position < 10),
            Committee::Verifier => assert!(proof.position >= 10 && proof.position < 37),
            Committee::Trainer => assert!(proof.position >= 37),
        }
    }
}

#[test]
fn test_get_witness() {
    let cs = CommitteeSelection::new(10, 20, 30, 100, 12345).unwrap();

    // Test for all possible indexes
    for i in 0..100 {
        let proof = cs.get_witness(i);
        assert!(proof.position < 100);

        // Verify that the witness status matches the position
        if proof.witness.is_true() {
            assert!(proof.position < 20);
        } else {
            assert!(proof.position >= 20);
        }
    }
}

#[test]
fn test_verify_committee() {
    let cs = CommitteeSelection::new(10, 20, 30, 100, 12345).unwrap();

    for i in 0..100 {
        let proof = cs.get_committee(i);
        assert!(cs.verify_committee(&proof));

        // Test with incorrect proof
        let incorrect_proof = CommitteeProof {
            committee: Committee::Verifier,
            position: 99,
            index: i,
        };
        assert!(!cs.verify_committee(&incorrect_proof));
    }
}

#[test]
fn test_verify_witness() {
    let cs = CommitteeSelection::new(10, 20, 30, 100, 12345).unwrap();

    for i in 0..100 {
        let proof = cs.get_witness(i);
        assert!(cs.verify_witness(&proof));

        // Test with incorrect proof
        let incorrect_proof = WitnessProof {
            witness: !proof.witness,
            position: 99,
            index: i,
        };
        assert!(!cs.verify_witness(&incorrect_proof));
    }
}

#[test]
fn test_committee_distribution() {
    let cs = CommitteeSelection::new(10, 20, 30, 100, 12345).unwrap();
    let mut tie_breaker_count = 0;
    let mut verifier_count = 0;
    let mut trainer_count = 0;

    for i in 0..100 {
        match cs.get_committee(i).committee {
            Committee::TieBreaker => tie_breaker_count += 1,
            Committee::Verifier => verifier_count += 1,
            Committee::Trainer => trainer_count += 1,
        }
    }

    assert_eq!(tie_breaker_count, 10);
    assert_eq!(verifier_count, 27);
    assert_eq!(trainer_count, 63);
}

#[test]
fn test_witness_distribution() {
    let cs = CommitteeSelection::new(10, 20, 30, 100, 12345).unwrap();
    let mut witness_count = 0;

    for i in 0..100 {
        if cs.get_witness(i).witness.is_true() {
            witness_count += 1;
        }
    }

    assert_eq!(witness_count, 20);
}

#[test]
fn test_get_num_nodes() {
    let cs = CommitteeSelection::new(10, 5, 20, 100, 12345).unwrap();
    assert_eq!(cs.get_num_tie_breaker_nodes(), 10);
    assert_eq!(cs.get_num_verifier_nodes(), 18);
    assert_eq!(cs.get_num_trainer_nodes(), 72);
}

#[test]
fn test_seed_consistency() {
    let cs1 = CommitteeSelection::new(10, 5, 20, 100, 12345).unwrap();
    let cs2 = CommitteeSelection::new(10, 5, 20, 100, 12345).unwrap();
    assert_eq!(cs1.get_seed(), cs2.get_seed());
}

#[test]
fn test_invalid_total_nodes() {
    assert!(CommitteeSelection::new(10, 5, 20, 9, 12345).is_err());
}

#[test]
fn test_invalid_comittee_selections() {
    // verification_percent > 100
    assert!(CommitteeSelection::new(10, 5, 101, 100, 12345).is_err());
    // total_nodes < tie_breaker_nodes
    assert!(CommitteeSelection::new(10, 5, 101, 5, 12345).is_err());
    // total_nodes < witness_nodes
    assert!(CommitteeSelection::new(10, 50, 101, 11, 12345).is_err());
    // total_nodes >= u64::MAX
    assert!(CommitteeSelection::new(10, 50, 101, u64::MAX as usize, 12345).is_err());
}

#[test]
fn test_edge_case_all_tie_breakers() {
    let cs = CommitteeSelection::new(100, 5, 20, 100, 12345).unwrap();
    for i in 0..100 {
        let committee = cs.get_committee(i).committee;
        assert_eq!(committee, Committee::TieBreaker);
    }
}

#[test]
fn test_edge_case_no_verifiers() {
    let cs = CommitteeSelection::new(10, 5, 0, 100, 12345).unwrap();
    let mut tie_breaker_count = 0;
    let mut trainer_count = 0;
    for i in 0..100 {
        let committee = cs.get_committee(i).committee;
        match committee {
            Committee::TieBreaker => tie_breaker_count += 1,
            Committee::Trainer => trainer_count += 1,
            _ => panic!("Unexpected committee type"),
        }
    }
    assert_eq!(tie_breaker_count, 10);
    assert_eq!(trainer_count, 90);
}
