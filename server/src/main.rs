#[macro_use]
extern crate rocket;
extern crate diesel;

use rocket::fairing::{Fairing, Info, Kind};
use rocket::{Request, Data, Rocket};
use rocket::http::Status;

mod chunk_id;
mod chunks;
mod metadata;
mod models;
mod schema;
mod auth;

struct AuthFairing;

// #[rocket::async_trait]
// impl Fairing for AuthFairing {
//     fn info(&self) -> Info {
//         Info {
//             name: "JWT Authentication Fairing",
//             kind: Kind::Request
//         }
//     }

//     async fn on_request(&self, request: &mut Request<'_>, _: &mut Data<'_>) {
//         // Skip auth for specific routes or methods if necessary
//         if request.uri().path() == "/login" {
//             return;
//         }

//         let token_valid = if let Some(auth_header) = request.headers().get_one("Authorization") {
//             auth::validate_token(auth_header).is_ok()
//         } else {
//             false
//         };

//         if !token_valid {
//             eprintln!("Unauthorized request");
//             request.set_uri("/auth_failed"); // redirect to an error handling route
//         }
//     }
// }

#[get("/auth_failed")]
fn auth_failed() -> Status {
    Status::Unauthorized
}

#[launch]
fn rocket() -> _ {
    rocket::build()
        // .attach(AuthFairing)
        // .mount("/", routes![auth_failed])
        .attach(chunks::stage())
        .attach(metadata::stage())
}
