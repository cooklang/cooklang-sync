use std::io;

use rocket::data::{Data, ToByteUnit};
use rocket::fairing::AdHoc;
use rocket::http::uri::Absolute;
use rocket::response::content::RawText;
use rocket::tokio::fs::File;

use crate::chunk_id::ChunkId;

// In a real application, these would be retrieved dynamically from a config.
const HOST: Absolute<'static> = uri!("http://localhost:8000");

/// Uploads chunk to a storage
#[post("/<id>", data = "<chunk>")]
async fn upload(id: ChunkId<'_>, chunk: Data<'_>) -> io::Result<String> {
    chunk
        .open(128.kibibytes())
        .into_file(id.file_path())
        .await?;
    Ok(uri!(HOST, retrieve(id)).to_string())
}


/// Downloads chunk from a storage
#[get("/<id>")]
async fn retrieve(id: ChunkId<'_>) -> Option<RawText<File>> {
    File::open(id.file_path()).await.map(RawText).ok()
}

pub fn stage() -> AdHoc {
    AdHoc::on_ignite("Chunk Server Stage", |rocket| async {
        rocket.mount("/chunks", routes![upload, retrieve])
    })
}
