use guardian_advisor::{redact_secrets, SendPermit, Skip};

// @kotowari[REQ-advisor-009, EX-advisor-017, EX-advisor-018]
#[test]
fn final_utf8_json_body_is_checked_at_the_exact_byte_boundary_without_truncation() {
    let body = format!(
        "{{\"model\":\"fixture-model\",\"text\":\"{}\"}}",
        "x".repeat(65501)
    )
    .into_bytes();
    assert_eq!(body.len(), 65536);
    let permit = SendPermit::checked(body.clone(), "fixture-model".into(), 65536).unwrap();
    assert_eq!(permit.body(), body);
    let oversized = format!(
        "{{\"model\":\"fixture-model\",\"text\":\"{}\"}}",
        "x".repeat(65502)
    )
    .into_bytes();
    assert!(matches!(
        SendPermit::checked(oversized, "fixture-model".into(), 65536),
        Err(Skip::Size)
    ));
    assert!(matches!(
        SendPermit::checked(vec![255], "fixture-model".into(), 65536),
        Err(Skip::Encoding)
    ));
}

// @kotowari[REQ-advisor-009, EX-advisor-018]
#[test]
fn secrets_in_any_encoded_field_or_escaped_string_prevent_a_send_and_are_redacted() {
    let fake = "sk-fixtureabcdefghijklmnopqrstuvwxyz";
    for field in ["command", "cwd", "reasons", "context", "model", "questions"] {
        let body = format!("{{\"{field}\":\"{fake}\"}}").into_bytes();
        assert!(matches!(
            SendPermit::checked(body, "fixture-model".into(), 65536),
            Err(Skip::Secret)
        ));
    }
    let encoded = br#"{"context":"\u0073\u006b-fixtureabcdefghijklmnopqrstuvwxyz"}"#.to_vec();
    assert!(matches!(
        SendPermit::checked(encoded, "fixture-model".into(), 65536),
        Err(Skip::Secret)
    ));
    let masked = redact_secrets(&format!("before {fake} after password=fixture-value"));
    assert!(!masked.contains(fake));
    assert!(!masked.contains("fixture-value"));
    assert!(masked.contains("before"));
}

// @kotowari[REQ-advisor-009, EX-advisor-018]
#[test]
fn quoted_credential_assignments_and_json_credential_fields_are_also_secrets() {
    for body in [
        r#"{"context":"{\"api_key\":\"fixture-private-value\"}"}"#,
        r#"{"password":"fixture-private-value"}"#,
        r#"{"\u0061pi_key":"fixture-private-value"}"#,
    ] {
        assert!(matches!(
            SendPermit::checked(body.as_bytes().to_vec(), "fixture-model".into(), 65536),
            Err(Skip::Secret)
        ));
        let masked = redact_secrets(body);
        assert!(!masked.contains("fixture-private-value"));
    }
}

// @kotowari[REQ-advisor-011, EX-advisor-022]
#[test]
fn detected_json_keys_and_nested_values_are_masked_without_colliding_with_plaintext_keys() {
    let fake = "sk-fixtureabcdefghijklmnopqrstuvwxyz";
    for body in [
        format!(r#"{{"{fake}":"fixture-key-value","[REDACTED]":"safe"}}"#),
        format!(r#"{{"nested":[{{"{fake}":"fixture-key-value"}}],"value":"{fake}"}}"#),
        format!(r#"{{"nested":{{"password":"fixture-private-value","value":"{fake}"}}}}"#),
    ] {
        let masked = redact_secrets(&body);
        assert!(!masked.contains(fake));
        assert!(!masked.contains("fixture-private-value"));
        serde_json::from_str::<serde_json::Value>(&masked).unwrap();
    }
}
