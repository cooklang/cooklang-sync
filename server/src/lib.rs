#[cfg(all(feature = "database_sqlite", feature = "database_postgres"))]
compile_error!(
    "feature \"database_sqlite\" and feature \"database_postgres\" cannot be enabled at the same time"
);

#[macro_use]
extern crate rocket;
extern crate diesel;

pub mod chunks;
pub mod metadata;
mod auth;
mod chunk_id;

pub fn create_server() -> rocket::Rocket<rocket::Build> {
    rocket::build()
        .attach(chunks::stage())
        .attach(metadata::stage())
}
