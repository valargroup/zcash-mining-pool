use pool_core::difficulty::{difficulty_to_target_hex, VardiffTracker};
use pool_core::share::parse_target;

#[test]
fn test_parse_target_full_length() {
    let target = parse_target(
        "0007ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
    )
    .unwrap();
    assert_eq!(target[0], 0x00);
    assert_eq!(target[1], 0x07);
    for &b in &target[2..] {
        assert_eq!(b, 0xff);
    }
}

#[test]
fn test_parse_target_short_padded() {
    let target = parse_target("07ff").unwrap();
    assert_eq!(target[30], 0x07);
    assert_eq!(target[31], 0xff);
    for &b in &target[..30] {
        assert_eq!(b, 0x00);
    }
}

#[test]
fn test_difficulty_target_roundtrip() {
    let target_hex = difficulty_to_target_hex(1.0);
    assert_eq!(target_hex.len(), 64);
    let target = parse_target(&target_hex).unwrap();
    // Difficulty 1 should produce a high target (near powLimit)
    assert!(target[0] > 0 || target[1] > 0);
}

#[test]
fn test_difficulty_target_higher_diff() {
    let easy = difficulty_to_target_hex(1.0);
    let hard = difficulty_to_target_hex(100.0);
    // Higher difficulty = lower target
    let easy_bytes = parse_target(&easy).unwrap();
    let hard_bytes = parse_target(&hard).unwrap();

    // Compare big-endian
    let easy_first_nonzero = easy_bytes.iter().position(|&b| b > 0).unwrap_or(32);
    let hard_first_nonzero = hard_bytes.iter().position(|&b| b > 0).unwrap_or(32);
    assert!(hard_first_nonzero >= easy_first_nonzero);
}

#[test]
fn test_vardiff_initial_state() {
    let v = VardiffTracker::new(10.0, 30.0, 1.0);
    assert_eq!(v.current_difficulty(), 1.0);
}

#[test]
fn test_vardiff_no_retarget_immediately() {
    let mut v = VardiffTracker::new(10.0, 30.0, 1.0);
    // A single share right away shouldn't trigger retarget
    assert!(v.record_share().is_none());
}
