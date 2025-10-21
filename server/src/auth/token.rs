use jsonwebtoken::{decode, errors::ErrorKind, Algorithm, DecodingKey, Validation};

use rocket::serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub uid: i32, // subject (who the token refers to)
    exp: usize,   // expiry date
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
