
use rocket::http::Status;
use rocket::request::{self, FromRequest, Outcome, Request};



use super::user::User;
use super::token::decode_token;

#[rocket::async_trait]
impl<'r> FromRequest<'r> for User {
    type Error = ();

    async fn from_request(request: &'r Request<'_>) -> request::Outcome<Self, Self::Error> {
        // println!("{:?}", create_token(100));

        if let Some(auth_header) = request.headers().get_one("Authorization") {
            if let Some(token) = auth_header.strip_prefix("Bearer ") {
                if let Ok(claim) = decode_token(token) {
                    return Outcome::Success(User { id: claim.uid });
                }
            }
        }

        Outcome::Error((Status::Unauthorized, ()))
    }
}
