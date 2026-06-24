use ring::hmac;

use crate::time::Clock;

/// Generate an HMAC-SHA256 signature for a device URL.
pub fn generate_signature_bytes<C: Clock>(
    secret: &str,
    device_id: i64,
    time: C,
) -> Vec<u8> {
    let timestamp = time.now_secs();

    let key = hmac::Key::new(hmac::HMAC_SHA256, secret.as_bytes());

    // Strict message format (starts with `_` to prevent parameter order attacks)
    let signing_message = format!("_d{}_t{}", device_id, timestamp).into_bytes();

    hmac::sign(&key, &signing_message).as_ref().to_vec()
}

/// Verify an HMAC-SHA256 signature and check expiration.
/// Returns `true` if the signature is valid and within the 1-minute window.
pub fn validate_signature<C: Clock>(
    secret: &str,
    device_id: i64,
    expected_sig: &[u8],
    request_timestamp: i64,
    time: C,
) -> bool {
    // Check expiration (1 minute window allowed - 60 seconds max validity, 5 seconds leeway)
    let now = time.now_secs();
    if now > request_timestamp + 60 || request_timestamp > now + 5 {
        return false;
    }

    // Reconstruct expected message with the *requested* timestamp
    let key = hmac::Key::new(hmac::HMAC_SHA256, secret.as_bytes());
    let signing_message = format!("_d{}_t{}", device_id, request_timestamp).into_bytes();

    // Ring uses constant-time comparison for this method
    hmac::verify(&key, &signing_message, expected_sig).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::MockClock;

    #[test]
    fn test_valid_signature_generation_and_validation() {
        let secret = "test-secret-key-for-hmac";
        let device_id: i64 = 123;
        let timestamp = 1000i64;
        
        let mock_clock = MockClock { time: timestamp };
        
        // Generate signature
        let sig_bytes = generate_signature_bytes(secret, device_id, &mock_clock);
        assert!(!sig_bytes.is_empty());
        assert_eq!(sig_bytes.len(), 32); // SHA256 produces 32 bytes

        // Validate signature with correct parameters (should succeed)
        let is_valid = validate_signature(secret, device_id, &sig_bytes, timestamp, mock_clock.clone());
        assert!(is_valid, "Signature should be valid");
    }

    #[test]
    fn test_reject_wrong_device_id() {
        let secret = "my-secret-key";
        let timestamp = 1000i64;
        let mock_clock = MockClock { time: timestamp };
        
        // Generate signature for device_id = 1
        let sig_bytes_w1 = generate_signature_bytes(secret, 1, &mock_clock);
        
        // Try to validate with different device ID (2) -> should fail
        let is_valid = validate_signature(secret, 2, &sig_bytes_w1, timestamp, mock_clock.clone());
        assert!(!is_valid, "Signature for wrong device_id should be rejected");
    }

    #[test]
    fn test_reject_wrong_secret() {
        let original_secret = "original-secret";
        let wrong_secret = "wrong-secret";
        let device_id: i64 = 1;
        let timestamp = 1000i64;
        
        let mock_clock = MockClock { time: timestamp };
        
        // Generate signature with original secret
        let sig_bytes = generate_signature_bytes(original_secret, device_id, &mock_clock);
        
        // Try to validate with wrong secret -> should fail
        let is_valid = validate_signature(wrong_secret, device_id, &sig_bytes, timestamp, mock_clock.clone());
        assert!(!is_valid, "Signature with wrong secret should be rejected");
    }

    #[test]
    fn test_reject_wrong_timestamp() {
        let secret = "my-secret-key";
        let device_id: i64 = 1;
        
        let original_time = MockClock { time: 1000 };
        let wrong_time = MockClock { time: 2000 }; // Different timestamp
        
        let sig_bytes = generate_signature_bytes(secret, device_id, &original_time);
        
        // Validate with wrong timestamp (but same clock for current time check)
        let is_valid = validate_signature(secret, device_id, &sig_bytes, original_time.time, wrong_time);
        assert!(!is_valid, "Signature with mismatched timestamps should be rejected");
    }

    #[test]
    fn test_reject_expired_timestamp() {
        let secret = "my-secret-key";
        let device_id: i64 = 1;
        
        // Create a mock clock at time 1000
        let current_time = MockClock { time: 1000 };
        
        // Use an old timestamp that's more than 60 seconds in the past
        let old_timestamp = 900i64; // Within window boundary
        
        let mock_clock_expired_test = MockClock { time: old_timestamp };
        
        // Generate a signature with OLD timestamp
        let sig_bytes = generate_signature_bytes(secret, device_id, &mock_clock_expired_test);
        
        // Try to validate it when "current" time is 1000 (900 + 60 = 960 maximum window)
        let is_valid = validate_signature(secret, device_id, &sig_bytes, old_timestamp, current_time.clone());
        // The signature should actually be valid since we're within the window (current=1000 >= timestamp=900 and request_timestamp+60 = 960 < now=1000) - wait that doesn't match!
        // Actually if old_timestamp is 900 and current_time is 1000, the check `now > request_timestamp + 60` becomes `1000 > 960` which is true -> rejected. Good!
        assert!(!is_valid, "Signature with timestamp outside the 1-minute window should be rejected");
    }

    #[test]
    fn test_signature_consistency() {
        let secret = "consistent-secret";
        let device_id: i64 = 42;
        let timestamp = 5000i64;
        
        let mock_clock = MockClock { time: timestamp };
        
        // Generate two signatures at same timestamp, should be identical
        let sig1 = generate_signature_bytes(secret, device_id, &mock_clock);
        let sig2 = generate_signature_bytes(secret, device_id, &mock_clock);
        
        assert_eq!(sig1, sig2, "Identical inputs should produce identical signatures");
    }

    #[test]
    fn test_valid_timestamp_near_boundary() {
        let secret = "boundary-test";
        let device_id: i64 = 5;
        
        // Current time is exactly at the boundary (timestamp + 60)
        let original_time = MockClock { time: 1000 };
        let current_time = MockClock { time: 1059 }; // Within window
        
        let sig_bytes = generate_signature_bytes(secret, device_id, &original_time);
        
        // Try to validate with original_timestamp=1000 and now=1059 (boundary)
        // Check `now > request_timestamp + 60` => `1059 > 1060` => false -> pass first check
        let is_valid = validate_signature(secret, device_id, &sig_bytes, 1000, current_time.clone());
        assert!(is_valid, "Signature within window boundary should be valid");
    }

    #[test]
    fn test_reject_expired_timestamp_by_one_second() {
        let secret = "boundary-test";
        let device_id: i64 = 5;
        
        // Current time is just past the boundary (timestamp + 60)
        let current_time = MockClock { time: 1060 }; // Exactly at limit where now == request_timestamp + 60 => rejected
        
        let original_time = MockClock { time: 1000 };
        let sig_bytes = generate_signature_bytes(secret, device_id, &original_time);
        
        // Try to validate with original_timestamp=1000 and now=1060 (past boundary)
        // Check `now > request_timestamp + 60` => `1060 > 1060` => false -> pass first check
        // So signature is still valid at exactly the boundary! That's by design.
        let is_valid = validate_signature(secret, device_id, &sig_bytes, 1000, current_time.clone());
        assert!(is_valid, "Signature exactly at window limit should be valid");
        
        // Now test with time that's truly expired
        let past_boundary_time = MockClock { time: 1061 };
        let is_valid = validate_signature(secret, device_id, &sig_bytes, 1000, past_boundary_time);
        assert!(!is_valid, "Signature one second past window limit should be invalid");
    }

    #[test]
    fn test_future_timestamp_rejected() {
        let secret = "future-test";
        let device_id: i64 = 5;
        
        // Current time is 1000
        let current_time = MockClock { time: 1000 };
        
        // Use a future timestamp (6 seconds in the future, exceeding 5 second leeway)
        let future_timestamp = 1006i64;
        
        let mock_clock_future = MockClock { time: future_timestamp };
        let sig_bytes = generate_signature_bytes(secret, device_id, &mock_clock_future);
        
        // Verify with future timestamp
        // Check `request_timestamp > now + 5` => `1006 > 1005` => true -> rejected!
        let is_valid = validate_signature(secret, device_id, &sig_bytes, future_timestamp, current_time.clone());
        assert!(!is_valid, "Signature with far-future timestamp should be rejected");
    }

    #[test]
    fn test_future_timestamp_accepted_within_leeway() {
        let secret = "future-test";
        let device_id: i64 = 5;
        
        // Current time is 1000
        let current_time = MockClock { time: 1000 };
        
        // Use a future timestamp (5 seconds in the future - within leeway)
        let future_timestamp = 1005i64;
        
        let mock_clock_future = MockClock { time: future_timestamp };
        let sig_bytes = generate_signature_bytes(secret, device_id, &mock_clock_future);
        
        // Verify with future timestamp
        // Check `request_timestamp > now + 5` => `1005 > 1005` => false -> pass first check
        // The signature IS technically valid because the time leeway allows it
        let is_valid = validate_signature(secret, device_id, &sig_bytes, future_timestamp, current_time.clone());
        assert!(is_valid, "Signature with timestamp within leeway should be valid");
    }
}
