use super::*;

#[test]
fn sha256_accepts_hex_and_rejects_invalid_encodings() {
    let checksum: Sha256Checksum = "AB".repeat(32).parse().unwrap();
    assert_eq!(checksum.to_string(), "ab".repeat(32));
    let json = serde_json::to_string(&checksum).unwrap();
    assert_eq!(
        serde_json::from_str::<Sha256Checksum>(&json).unwrap(),
        checksum
    );
    for invalid in [
        String::new(),
        "a".repeat(63),
        "a".repeat(65),
        "g".repeat(64),
        "?".repeat(32),
        format!(" {}", "a".repeat(64)),
    ] {
        assert!(invalid.parse::<Sha256Checksum>().is_err());
        assert!(serde_json::from_value::<Sha256Checksum>(serde_json::json!(invalid)).is_err());
    }
}

#[test]
fn sha256_verification_matches_known_vectors_and_detects_changes() {
    for (data, expected) in [
        (
            &b""[..],
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        ),
        (
            &b"abc"[..],
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        ),
    ] {
        let expected = expected.parse().unwrap();
        let mut verifier = ChecksumVerifier::new(&expected);
        for byte in data {
            verifier.update(&[*byte]);
        }
        verifier.verify().unwrap();
        let mut corrupt = ChecksumVerifier::new(&expected);
        corrupt.update(b"changed");
        let error = corrupt.verify().unwrap_err().to_string();
        assert!(error.contains("SHA-256 mismatch") && error.contains(&expected.to_string()));
    }
}
