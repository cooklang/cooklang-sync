//! JSON error bodies for the status codes the auth guards return.
//!
//! `EntitledUser` (see `auth::entitlement`) rejects with 402 when
//! `SYNC_ENFORCEMENT=enforce` and the account's sync entitlement is
//! missing/expired; `User` and `EntitledUser` both reject with 401 when the
//! bearer token itself is missing/invalid/expired. Without these catchers
//! Rocket would fall back to its default plain-text error pages.

use rocket::serde::json::{json, Json, Value};
use rocket::{Catcher, Request};

fn sync_upgrade_url() -> String {
    std::env::var("SYNC_UPGRADE_URL").unwrap_or_else(|_| "https://cook.md/pricing".to_string())
}

#[catch(402)]
fn payment_required(_req: &Request) -> Json<Value> {
    Json(json!({
        "error": "sync_requires_plan",
        "message": "Sync needs a Cook Basic or Pro plan. Accounts from before the paywall sync free.",
        "upgrade_url": sync_upgrade_url(),
    }))
}

#[catch(401)]
fn unauthorized(_req: &Request) -> Json<Value> {
    Json(json!({ "error": "unauthorized" }))
}

/// The 402/401 catchers. `chunks::stage()` already registers these against
/// the request base ("/") on ignite, so any deployment that attaches
/// `chunks::stage()` (as `create_server()` and the cook.md sync-server's
/// `main.rs` both do) gets them automatically.
///
/// This function exists only for a consumer that mounts routes through
/// *neither* `chunks::stage()` nor `metadata::stage()` (e.g. a bespoke
/// Rocket build using the guards directly). Do **not** call this alongside
/// `chunks::stage()`: Rocket aborts launch on a duplicate catcher
/// registration for the same (status, base) pair, so a consumer that
/// attaches `chunks::stage()` must not also register these itself.
pub fn catchers() -> Vec<Catcher> {
    catchers![payment_required, unauthorized]
}
