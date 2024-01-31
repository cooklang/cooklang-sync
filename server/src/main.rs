#[macro_use] extern crate rocket;

mod chunks;

use std::io;

use rocket::data::{Data, ToByteUnit};
use rocket::http::uri::Absolute;
use rocket::response::content::RawText;
use rocket::tokio::fs::{self, File};

#[launch]
fn rocket() -> _ {
    rocket::build().attach(chunks::stage())
}
