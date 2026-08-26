//! Integration tests for the `EntitledUser` guard's actual Rocket wiring:
//! that `chunks::stage()` really does register the 401/402 catchers, and
//! that the guard's off/enforce decision really does gate the request
//! before the handler runs -- over real HTTP, via `rocket::local::blocking`.
//!
//! Deliberately uses only `chunks::stage()` (no `metadata::stage()`), since
//! `chunks` routes touch neither the DB fairing nor the filesystem until
//! *after* the guard has already let a request through, so these tests need
//! no sqlite file or migrations.

use std::sync::Mutex;

use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use rocket::http::{Header as HttpHeader, Status};
use rocket::local::blocking::Client;
use rocket::serde::json::{json, Value};

const JWT_SECRET: &str = "entitlement-catchers-test-secret";

/// `JWT_SECRET` and `SYNC_ENFORCEMENT` are process-wide env vars read fresh
/// on every request (see `auth::request::secret` and
/// `auth::entitlement::enforcement_mode`). This file is its own test
/// binary/process (cargo builds each `tests/*.rs` file separately), so this
/// lock only needs to serialize the handful of tests *within this file*
/// against each other -- it has no interaction with unit tests elsewhere in
/// the crate.
static ENV_LOCK: Mutex<()> = Mutex::new(());

/// Acquires `ENV_LOCK`, recovering from poisoning instead of propagating it.
/// A panicking assertion inside one test (e.g. a status-code mismatch) would
/// otherwise poison the lock and cascade-fail every other test in this file
/// via `.unwrap()` on the next `.lock()`, obscuring the real failure. The
/// guarded env vars are always fully re-set at the top of each test, so a
/// stale value left behind by a panicked test is never actually observed.
fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn far_future_exp() -> usize {
    (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 3600) as usize
}

fn sign(payload: &Value) -> String {
    encode(
        &Header::new(Algorithm::HS256),
        payload,
        &EncodingKey::from_secret(JWT_SECRET.as_bytes()),
    )
    .unwrap()
}

/// Builds a `chunks::stage()`-only Rocket instance and runs it through
/// ignite (which is where the catchers get registered and the startup mode
/// log line fires). `JWT_SECRET` must already be set by the caller before
/// this runs, since a request carrying a bearer token reads it.
fn client() -> Client {
    let rocket = rocket::build().attach(cooklang_sync_server::chunks::stage());
    Client::tracked(rocket).expect("valid rocket instance")
}

#[test]
fn request_without_a_token_is_rejected_with_401_and_the_contract_body() {
    let _guard = env_lock();
    std::env::set_var("JWT_SECRET", JWT_SECRET);
    std::env::remove_var("SYNC_ENFORCEMENT"); // mode is irrelevant: no token, no claim to judge

    let client = client();
    let response = client.get("/chunks/doesnotexist12345").dispatch();

    assert_eq!(response.status(), Status::Unauthorized);
    let body: Value = response
        .into_json()
        .expect("catcher must return a JSON body");
    assert_eq!(body, json!({ "error": "unauthorized" }));
}

#[test]
fn missing_entitlement_under_enforce_is_rejected_with_402_and_the_contract_body() {
    let _guard = env_lock();
    std::env::set_var("JWT_SECRET", JWT_SECRET);
    std::env::set_var("SYNC_ENFORCEMENT", "enforce");

    let client = client();
    // Valid signature, valid (non-expired) token -- but no `sync_until`
    // claim at all, i.e. a non-entitled (or pre-entitlement) account.
    let token = sign(&json!({ "uid": 1, "exp": far_future_exp() }));

    let response = client
        .get("/chunks/doesnotexist12345")
        .header(HttpHeader::new("Authorization", format!("Bearer {token}")))
        .dispatch();

    assert_eq!(response.status(), Status::PaymentRequired);
    let body: Value = response
        .into_json()
        .expect("catcher must return a JSON body");
    assert_eq!(
        body,
        json!({
            "error": "sync_requires_plan",
            "message": "Sync needs a Cook Basic or Pro plan. Accounts from before the paywall sync free.",
            "upgrade_url": "https://cook.md/pricing",
        })
    );

    std::env::remove_var("SYNC_ENFORCEMENT");
}

#[test]
fn missing_entitlement_under_off_is_not_blocked_by_the_guard() {
    let _guard = env_lock();
    std::env::set_var("JWT_SECRET", JWT_SECRET);
    std::env::remove_var("SYNC_ENFORCEMENT"); // default is "off"

    let client = client();
    // Same claim shape as the enforce case above: valid token, no
    // `sync_until` at all.
    let token = sign(&json!({ "uid": 1, "exp": far_future_exp() }));

    let response = client
        .get("/chunks/doesnotexist12345")
        .header(HttpHeader::new("Authorization", format!("Bearer {token}")))
        .dispatch();

    // "off" never rejects on entitlement grounds, so neither auth-related
    // status should appear here. The guard let the request through to
    // `chunks::retrieve`, which then looks for a chunk file that genuinely
    // doesn't exist on disk and 404s -- proving the guard's decision
    // without needing any DB fairing.
    assert_ne!(response.status(), Status::Unauthorized);
    assert_ne!(response.status(), Status::PaymentRequired);
    assert_eq!(response.status(), Status::NotFound);
}
