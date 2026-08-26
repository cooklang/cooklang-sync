use rocket::http::Status;
use rocket::request::{self, FromRequest, Outcome, Request};

use super::token::{decode_token, Claims};
use super::user::User;

fn secret() -> String {
    std::env::var("JWT_SECRET").expect("JWT_SECRET must be set.")
}

/// Pulls the `Authorization: Bearer <jwt>` header off `request` and decodes
/// it into `Claims`. Shared by both the plain `User` guard and the
/// entitlement-aware `EntitledUser` guard so they agree on what counts as
/// "authenticated" and so `EntitledUser` can see claims (like `sync_until`)
/// that `User` itself doesn't need.
pub(super) fn extract_claims(request: &Request<'_>) -> Result<Claims, ()> {
    let auth_header = request.headers().get_one("Authorization").ok_or(())?;
    let token = auth_header.strip_prefix("Bearer ").ok_or(())?;
    decode_token(token, secret().as_bytes())
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for User {
    type Error = ();

    async fn from_request(request: &'r Request<'_>) -> request::Outcome<Self, Self::Error> {
        match extract_claims(request) {
            Ok(claim) => Outcome::Success(User { id: claim.uid }),
            Err(_) => Outcome::Error((Status::Unauthorized, ())),
        }
    }
}
