use rocket::serde::{Serialize, Deserialize};
use rocket::request::{self, Outcome, FromRequest, Request};
use rocket::http::Status;
use jsonwebtoken::{encode, decode, Header, Algorithm, Validation, EncodingKey, DecodingKey, errors::ErrorKind};
use std::time::{SystemTime, UNIX_EPOCH};

const TOKEN_EXPIRATION_DAYS: u64 = 100;

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    uid: u64,  // subject (who the token refers to)
    exp: usize,   // expiry date
}

pub struct User {
    id: u64
}

pub fn create_token(user_id: u64) -> String {
    let expiration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_secs() + 60 * 24 * TOKEN_EXPIRATION_DAYS;

    let claims = Claims { uid: user_id, exp: expiration as usize };

    encode(&Header::default(), &claims, &EncodingKey::from_secret("secret".as_ref())).unwrap()
}

fn decode_token(token: &str) -> Result<Claims, ()> {
    match decode::<Claims>(
        token,
        &DecodingKey::from_secret("secret".as_ref()),
        &Validation::new(Algorithm::HS256)
    ) {
        Ok(c) => Ok(c.claims),
        Err(err) => match *err.kind() {
            ErrorKind::ExpiredSignature => Err(()), // Token is expired
            _ => Err(()), // Some other error
        },
    }
}

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
