use jsonwebtoken::{decode, errors::ErrorKind, Algorithm, DecodingKey, Validation};

use rocket::serde::{Deserialize, Deserializer, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub uid: i32, // subject (who the token refers to)
    exp: usize,   // expiry date
    /// Unix timestamp until which the account's sync entitlement is valid.
    /// Optional and unknown-claim-tolerant: older tokens (minted before the
    /// entitlement rollout) simply decode with `None` here, and any other
    /// unrecognized claims already fall out during deserialization because
    /// serde ignores unknown fields by default.
    ///
    /// `deserialize_with = "lenient_ts"` additionally makes a *present but
    /// malformed* claim (a string, float, bool, or explicit `null`) decode
    /// as `None` rather than failing the whole token: a malformed claim
    /// must degrade to "no entitlement claim" (allowed in `off`/`log`, 402
    /// in `enforce`), never to a 401 that takes the request down before the
    /// entitlement guard even runs.
    #[serde(default, deserialize_with = "lenient_ts")]
    pub sync_until: Option<i64>,
}

/// Deserializes `sync_until` leniently: `Some(n)` only for a JSON integer,
/// `None` for anything else that could show up in that slot (string, float,
/// bool, `null`) instead of erroring out. See the field doc comment on why
/// this must never fail the token decode.
fn lenient_ts<'de, D>(deserializer: D) -> Result<Option<i64>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<rocket::serde::json::Value>::deserialize(deserializer)?;
    Ok(value.and_then(|v| match v {
        rocket::serde::json::Value::Number(n) => n.as_i64(),
        _ => None,
    }))
}

pub fn decode_token(token: &str, secret: &[u8]) -> Result<Claims, ()> {
    match decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret),
        &Validation::new(Algorithm::HS256),
    ) {
        Ok(c) => Ok(c.claims),
        Err(err) => match *err.kind() {
            ErrorKind::ExpiredSignature => Err(()), // Token is expired
            _ => Err(()),                           // Some other error
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::{encode, EncodingKey, Header};

    const SECRET: &[u8] = b"test-secret";

    fn sign(claims: &Claims) -> String {
        encode(
            &Header::new(Algorithm::HS256),
            claims,
            &EncodingKey::from_secret(SECRET),
        )
        .unwrap()
    }

    /// Signs a JSON payload directly (bypassing `Claims`) so we can simulate
    /// a token minted before `sync_until` existed at all. Reuses rocket's
    /// re-exported `serde_json` (server already depends on rocket's "json"
    /// feature) instead of pulling in a fresh dev-dependency just for tests.
    fn sign_raw(json: &rocket::serde::json::Value) -> String {
        encode(
            &Header::new(Algorithm::HS256),
            json,
            &EncodingKey::from_secret(SECRET),
        )
        .unwrap()
    }

    fn far_future_exp() -> usize {
        (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 3600) as usize
    }

    #[test]
    fn decodes_token_without_sync_until_as_none() {
        let claims = Claims {
            uid: 42,
            exp: far_future_exp(),
            sync_until: None,
        };
        let token = sign(&claims);

        let decoded = decode_token(&token, SECRET).expect("decode should succeed");
        assert_eq!(decoded.uid, 42);
        assert_eq!(decoded.sync_until, None);
    }

    #[test]
    fn decodes_token_with_sync_until_present() {
        let claims = Claims {
            uid: 7,
            exp: far_future_exp(),
            sync_until: Some(1_900_000_000),
        };
        let token = sign(&claims);

        let decoded = decode_token(&token, SECRET).expect("decode should succeed");
        assert_eq!(decoded.uid, 7);
        assert_eq!(decoded.sync_until, Some(1_900_000_000));
    }

    #[test]
    fn decodes_legacy_token_with_no_sync_until_field_at_all() {
        // Simulates a JWT minted before the `sync_until` claim was
        // introduced: the field is entirely absent from the payload, not
        // just `null`. `#[serde(default)]` must tolerate that.
        let payload = rocket::serde::json::json!({
            "uid": 13,
            "exp": far_future_exp(),
        });
        let token = sign_raw(&payload);

        let decoded = decode_token(&token, SECRET).expect("decode should succeed");
        assert_eq!(decoded.uid, 13);
        assert_eq!(decoded.sync_until, None);
    }

    #[test]
    fn decodes_token_tolerating_unknown_extra_claims() {
        // Any other unrecognized claim (present or future) must not break
        // decoding for either User or EntitledUser.
        let payload = rocket::serde::json::json!({
            "uid": 21,
            "exp": far_future_exp(),
            "sync_until": 1_900_000_000,
            "some_future_claim": "whatever",
        });
        let token = sign_raw(&payload);

        let decoded = decode_token(&token, SECRET).expect("decode should succeed");
        assert_eq!(decoded.uid, 21);
        assert_eq!(decoded.sync_until, Some(1_900_000_000));
    }

    #[test]
    fn decodes_string_sync_until_as_none_instead_of_failing() {
        let payload = rocket::serde::json::json!({
            "uid": 30,
            "exp": far_future_exp(),
            "sync_until": "not-a-timestamp",
        });
        let token = sign_raw(&payload);

        let decoded = decode_token(&token, SECRET)
            .expect("a malformed sync_until must degrade to None, not fail decode");
        assert_eq!(decoded.uid, 30);
        assert_eq!(decoded.sync_until, None);
    }

    #[test]
    fn decodes_float_sync_until_as_none_instead_of_failing() {
        let payload = rocket::serde::json::json!({
            "uid": 31,
            "exp": far_future_exp(),
            "sync_until": 1_900_000_000.5,
        });
        let token = sign_raw(&payload);

        let decoded = decode_token(&token, SECRET)
            .expect("a malformed sync_until must degrade to None, not fail decode");
        assert_eq!(decoded.uid, 31);
        assert_eq!(decoded.sync_until, None);
    }

    #[test]
    fn decodes_bool_sync_until_as_none_instead_of_failing() {
        let payload = rocket::serde::json::json!({
            "uid": 32,
            "exp": far_future_exp(),
            "sync_until": true,
        });
        let token = sign_raw(&payload);

        let decoded = decode_token(&token, SECRET)
            .expect("a malformed sync_until must degrade to None, not fail decode");
        assert_eq!(decoded.uid, 32);
        assert_eq!(decoded.sync_until, None);
    }

    #[test]
    fn decodes_null_sync_until_as_none_instead_of_failing() {
        let payload = rocket::serde::json::json!({
            "uid": 33,
            "exp": far_future_exp(),
            "sync_until": null,
        });
        let token = sign_raw(&payload);

        let decoded = decode_token(&token, SECRET).expect("decode should succeed");
        assert_eq!(decoded.uid, 33);
        assert_eq!(decoded.sync_until, None);
    }

    #[test]
    fn rejects_token_signed_with_wrong_secret() {
        let claims = Claims {
            uid: 1,
            exp: far_future_exp(),
            sync_until: None,
        };
        let token = sign(&claims);

        assert!(decode_token(&token, b"wrong-secret").is_err());
    }

    #[test]
    fn rejects_expired_token() {
        let claims = Claims {
            uid: 1,
            exp: 1, // long past
            sync_until: None,
        };
        let token = sign(&claims);

        assert!(decode_token(&token, SECRET).is_err());
    }
}
