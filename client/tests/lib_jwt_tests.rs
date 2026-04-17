//! Tests for `extract_uid_from_jwt`.
//!
//! `extract_uid_from_jwt` does **not** verify signatures — it only decodes the
//! middle (payload) segment and extracts the `uid` claim. These tests document
//! current behavior, including panic paths, so future refactors notice breakage.

mod common;

use cooklang_sync_client::extract_uid_from_jwt;

#[test]
fn extract_uid_returns_correct_uid_for_valid_token() {
    let token = common::sample_jwt(42);
    assert_eq!(extract_uid_from_jwt(&token), 42);
}

#[test]
fn extract_uid_handles_zero_uid() {
    let token = common::sample_jwt(0);
    assert_eq!(extract_uid_from_jwt(&token), 0);
}

#[test]
fn extract_uid_handles_negative_uid() {
    let token = common::sample_jwt(-1);
    assert_eq!(extract_uid_from_jwt(&token), -1);
}

#[test]
#[should_panic]
fn extract_uid_panics_on_token_with_missing_segments() {
    // Only one segment — no '.' separators.
    let _ = extract_uid_from_jwt("notatoken");
}

#[test]
#[should_panic]
fn extract_uid_panics_on_malformed_base64_payload() {
    // Three segments but payload is not valid base64.
    let _ = extract_uid_from_jwt("aaa.!!!not-b64!!!.bbb");
}

#[test]
#[should_panic]
fn extract_uid_panics_on_payload_without_uid_field() {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;
    let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"HS256","typ":"JWT"}"#);
    let payload = URL_SAFE_NO_PAD.encode(br#"{"not_uid":1}"#);
    let sig = URL_SAFE_NO_PAD.encode(b"x");
    let token = format!("{header}.{payload}.{sig}");
    let _ = extract_uid_from_jwt(&token);
}
