#[macro_use]
extern crate rocket;
extern crate diesel;

pub mod chunks;
pub mod metadata;
mod auth;
mod chunk_id;
mod db;
mod models;
mod schema;

pub fn create_server() -> rocket::Rocket<rocket::Build> {
    rocket::build()
        .attach(chunks::stage())
        .attach(metadata::stage())
}
