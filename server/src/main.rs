#[macro_use]
extern crate rocket;
extern crate diesel;

mod auth;
mod chunk_id;
mod chunks;
mod db;
mod metadata;
mod models;
mod schema;

// TODO need config for root of uploads and also secret for tokens
#[launch]
fn rocket() -> _ {
    rocket::build()
        .attach(chunks::stage())
        .attach(metadata::stage())
}
