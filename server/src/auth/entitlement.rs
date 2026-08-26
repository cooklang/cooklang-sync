use std::time::{SystemTime, UNIX_EPOCH};

use rocket::http::Status;
use rocket::request::{self, FromRequest, Outcome, Request};

use super::request::extract_claims;
use super::user::User;

/// How strictly the sync entitlement (the JWT's optional `sync_until` claim)
/// is enforced on `EntitledUser`-guarded routes. Controlled by the
/// `SYNC_ENFORCEMENT` env var, read once per request (cheap: just an env
/// lookup, no caching needed at this call volume).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EnforcementMode {
    /// Behave exactly like a plain `User` guard: never block, never log.
    Off,
    /// Never block, but log what *would* have been blocked under `Enforce`.
    Log,
    /// Reject requests whose claim is missing or expired with 402.
    Enforce,
}

fn enforcement_mode() -> EnforcementMode {
    match std::env::var("SYNC_ENFORCEMENT") {
        Ok(v) if v.eq_ignore_ascii_case("enforce") => EnforcementMode::Enforce,
        Ok(v) if v.eq_ignore_ascii_case("log") => EnforcementMode::Log,
        _ => EnforcementMode::Off,
    }
}

impl EnforcementMode {
    fn as_str(self) -> &'static str {
        match self {
            EnforcementMode::Off => "off",
            EnforcementMode::Log => "log",
            EnforcementMode::Enforce => "enforce",
        }
    }
}

/// The resolved `SYNC_ENFORCEMENT` mode, as a stable string, for a one-line
/// startup log so it's visible at a glance which mode a deployment is
/// actually running in (rather than only inferable from behavior at
/// request time). Called once from `chunks::stage()`'s ignite.
pub(crate) fn current_mode_name() -> &'static str {
    enforcement_mode().as_str()
}

/// Why a claim failed the entitlement check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EntitlementReason {
    /// The JWT carried no `sync_until` claim at all (e.g. an older token
    /// minted before the entitlement rollout, or a non-entitled account).
    MissingClaim,
    /// The JWT carried a `sync_until` claim, but it is in the past.
    ExpiredClaim,
}

impl EntitlementReason {
    fn as_str(self) -> &'static str {
        match self {
            EntitlementReason::MissingClaim => "missing_claim",
            EntitlementReason::ExpiredClaim => "expired_claim",
        }
    }
}

/// What an `EntitledUser` guard should do for a given (mode, claim, now)
/// combination. Factored out as a pure function of plain data so it's
/// testable without touching Rocket or the clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EntitlementDecision {
    /// Let the request through, no log line.
    Allow,
    /// Let the request through, but emit the "would block" log line.
    AllowWithWarning(EntitlementReason),
    /// Reject the request with 402.
    Reject(EntitlementReason),
}

/// Pure decision logic, independent of Rocket. `now` is a unix timestamp
/// (seconds), passed in rather than read from the clock so tests can control
/// it.
pub(super) fn decide(
    mode: EnforcementMode,
    sync_until: Option<i64>,
    now: i64,
) -> EntitlementDecision {
    let reason = match sync_until {
        None => Some(EntitlementReason::MissingClaim),
        Some(t) if t < now => Some(EntitlementReason::ExpiredClaim),
        Some(_) => None,
    };

    let Some(reason) = reason else {
        return EntitlementDecision::Allow;
    };

    match mode {
        EnforcementMode::Off => EntitlementDecision::Allow,
        EnforcementMode::Log => EntitlementDecision::AllowWithWarning(reason),
        EnforcementMode::Enforce => EntitlementDecision::Reject(reason),
    }
}

fn now_unix() -> i64 {
    // Fail closed: if the clock is broken badly enough that we're apparently
    // before the epoch, treat "now" as the largest possible timestamp rather
    // than 0. Every `sync_until` then reads as expired, which blocks in
    // `enforce` mode (and only logs in `log`/does nothing in `off`) instead
    // of a broken clock silently letting every claim through as "not yet
    // expired".
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(i64::MAX)
}

fn route_name(request: &Request<'_>) -> String {
    match request.route() {
        Some(route) => format!("{} {}", request.method(), route.uri),
        None => format!("{} {}", request.method(), request.uri()),
    }
}

/// A `User` that additionally satisfies the sync entitlement check, per the
/// current `SYNC_ENFORCEMENT` mode. In `off`/`log` mode this behaves exactly
/// like `User` from the handler's point of view (it never rejects); in
/// `enforce` mode a missing/expired `sync_until` claim yields a 402 instead
/// of the handler ever running.
///
/// Wraps `User` (rather than duplicating its fields) so `EntitledUser` stays
/// a thin decoration over the same identity, and `Deref`s to it so existing
/// handler bodies that read `user.id` keep working unchanged after swapping
/// the guard type in the signature.
pub struct EntitledUser(pub User);

impl std::ops::Deref for EntitledUser {
    type Target = User;

    fn deref(&self) -> &User {
        &self.0
    }
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for EntitledUser {
    type Error = ();

    async fn from_request(request: &'r Request<'_>) -> request::Outcome<Self, Self::Error> {
        let claim = match extract_claims(request) {
            Ok(c) => c,
            Err(_) => return Outcome::Error((Status::Unauthorized, ())),
        };

        let decision = decide(enforcement_mode(), claim.sync_until, now_unix());

        match decision {
            EntitlementDecision::Allow => Outcome::Success(EntitledUser(User { id: claim.uid })),
            EntitlementDecision::AllowWithWarning(reason) => {
                warn!(
                    "sync_entitlement_would_block uid={} route={} reason={}",
                    claim.uid,
                    route_name(request),
                    reason.as_str()
                );
                Outcome::Success(EntitledUser(User { id: claim.uid }))
            }
            EntitlementDecision::Reject(reason) => {
                warn!(
                    "sync_entitlement_blocked uid={} route={} reason={}",
                    claim.uid,
                    route_name(request),
                    reason.as_str()
                );
                Outcome::Error((Status::PaymentRequired, ()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_000_000;

    #[test]
    fn off_mode_always_allows_regardless_of_claim() {
        assert_eq!(
            decide(EnforcementMode::Off, None, NOW),
            EntitlementDecision::Allow
        );
        assert_eq!(
            decide(EnforcementMode::Off, Some(NOW - 1), NOW),
            EntitlementDecision::Allow
        );
        assert_eq!(
            decide(EnforcementMode::Off, Some(NOW + 1), NOW),
            EntitlementDecision::Allow
        );
    }

    #[test]
    fn valid_claim_is_always_allowed_in_every_mode() {
        for mode in [
            EnforcementMode::Off,
            EnforcementMode::Log,
            EnforcementMode::Enforce,
        ] {
            assert_eq!(decide(mode, Some(NOW + 1), NOW), EntitlementDecision::Allow);
        }
    }

    #[test]
    fn log_mode_allows_but_flags_missing_claim() {
        assert_eq!(
            decide(EnforcementMode::Log, None, NOW),
            EntitlementDecision::AllowWithWarning(EntitlementReason::MissingClaim)
        );
    }

    #[test]
    fn log_mode_allows_but_flags_expired_claim() {
        assert_eq!(
            decide(EnforcementMode::Log, Some(NOW - 1), NOW),
            EntitlementDecision::AllowWithWarning(EntitlementReason::ExpiredClaim)
        );
    }

    #[test]
    fn enforce_mode_rejects_missing_claim() {
        assert_eq!(
            decide(EnforcementMode::Enforce, None, NOW),
            EntitlementDecision::Reject(EntitlementReason::MissingClaim)
        );
    }

    #[test]
    fn enforce_mode_rejects_expired_claim() {
        assert_eq!(
            decide(EnforcementMode::Enforce, Some(NOW - 1), NOW),
            EntitlementDecision::Reject(EntitlementReason::ExpiredClaim)
        );
    }

    #[test]
    fn claim_valid_exactly_through_now_is_not_expired() {
        // The spec's rule is `sync_until < now` => expired, so a claim whose
        // `sync_until` equals `now` is still (just barely) valid.
        assert_eq!(
            decide(EnforcementMode::Enforce, Some(NOW), NOW),
            EntitlementDecision::Allow
        );
    }

    #[test]
    fn enforcement_mode_parses_env_var_case_insensitively_and_defaults_to_off() {
        // SAFETY (in the test-thread-safety sense): this is the only test in
        // the crate that reads or writes SYNC_ENFORCEMENT, and it does so
        // entirely within one test function, so there's no cross-test race
        // even though `cargo test` runs test functions concurrently.
        let restore = std::env::var("SYNC_ENFORCEMENT").ok();

        std::env::remove_var("SYNC_ENFORCEMENT");
        assert_eq!(enforcement_mode(), EnforcementMode::Off);

        std::env::set_var("SYNC_ENFORCEMENT", "enforce");
        assert_eq!(enforcement_mode(), EnforcementMode::Enforce);

        std::env::set_var("SYNC_ENFORCEMENT", "ENFORCE");
        assert_eq!(enforcement_mode(), EnforcementMode::Enforce);

        std::env::set_var("SYNC_ENFORCEMENT", "log");
        assert_eq!(enforcement_mode(), EnforcementMode::Log);

        std::env::set_var("SYNC_ENFORCEMENT", "off");
        assert_eq!(enforcement_mode(), EnforcementMode::Off);

        std::env::set_var("SYNC_ENFORCEMENT", "garbage");
        assert_eq!(enforcement_mode(), EnforcementMode::Off);

        match restore {
            Some(v) => std::env::set_var("SYNC_ENFORCEMENT", v),
            None => std::env::remove_var("SYNC_ENFORCEMENT"),
        }
    }
}
