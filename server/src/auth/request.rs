use rocket::http::Status;
use rocket::request::{self, FromRequest, Outcome, Request};

use super::token::decode_token;
use super::user::User;

fn secret() -> String {
    std::env::var("JWT_SECRET").expect("JWT_SECRET must be set.")
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for User {
    type Error = ();

    async fn from_request(request: &'r Request<'_>) -> request::Outcome<Self, Self::Error> {
        // println!("{:?}", crate::auth::token::create_token(99));

        if let Some(auth_header) = request.headers().get_one("Authorization") {
            if let Some(token) = auth_header.strip_prefix("Bearer ") {
                if let Ok(claim) = decode_token(token, secret().as_bytes()) {
                    return Outcome::Success(User { id: claim.uid });
                }
            }
        }

        Outcome::Error((Status::Unauthorized, ()))
    }
}
