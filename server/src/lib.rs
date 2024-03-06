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

pub fn create_server() -> rocket::Rocket<rocket::Build> {
    rocket::build()
        .attach(chunks::stage())
        .attach(metadata::stage())
}
