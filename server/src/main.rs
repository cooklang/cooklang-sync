#[macro_use]
extern crate rocket;

#[macro_use]
extern crate diesel;

mod chunks;
mod metadata;
mod models;
mod schema;

#[launch]
fn rocket() -> _ {
    rocket::build()
        .attach(chunks::stage())
        .attach(metadata::stage())
}
