use jsonwebtoken::{
    decode, encode, errors::ErrorKind, Algorithm, DecodingKey, EncodingKey, Header, Validation,
};

use rocket::serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

const TOKEN_EXPIRATION_DAYS: u64 = 100;

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub uid: u64, // subject (who the token refers to)
    exp: usize,   // expiry date
}

pub fn create_token(user_id: u64) -> String {
    let expiration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_secs()
        + 60 * 60 * 24 * TOKEN_EXPIRATION_DAYS;

    let claims = Claims {
        uid: user_id,
        exp: expiration as usize,
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret("secret".as_ref()),
    )
    .unwrap()
}

pub fn decode_token(token: &str) -> Result<Claims, ()> {
    match decode::<Claims>(
        token,
        &DecodingKey::from_secret("secret".as_ref()),
        &Validation::new(Algorithm::HS256),
    ) {
        Ok(c) => Ok(c.claims),
        Err(err) => match *err.kind() {
            ErrorKind::ExpiredSignature => Err(()), // Token is expired
            _ => Err(()),                           // Some other error
        },
    }
}
