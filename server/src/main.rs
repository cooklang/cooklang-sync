#[macro_use]
extern crate rocket;

mod chunks;
mod metadata;

#[launch]
fn rocket() -> _ {
    rocket::build()
        .attach(chunks::stage())
        .attach(metadata::stage())
}
