
#[cfg(test)]
mod tests;
mod chunk_id;

use std::io;

use rocket::fairing::AdHoc;
use rocket::data::{Data, ToByteUnit};
use rocket::http::uri::Absolute;
use rocket::response::content::RawText;
use rocket::tokio::fs::{self, File};

use chunk_id::ChunkId;

// In a real application, these would be retrieved dynamically from a config.
const HOST: Absolute<'static> = uri!("http://localhost:8000");

#[post("/<id>", data = "<chunk>")]
async fn upload(id: ChunkId<'_>, chunk: Data<'_>) -> io::Result<String> {
    chunk.open(128.kibibytes()).into_file(id.file_path()).await?;
    Ok(uri!(HOST, retrieve(id)).to_string())
}

#[get("/<id>")]
async fn retrieve(id: ChunkId<'_>) -> Option<RawText<File>> {
    File::open(id.file_path()).await.map(RawText).ok()
}

#[delete("/<id>")]
async fn delete(id: ChunkId<'_>) -> Option<()> {
    fs::remove_file(id.file_path()).await.ok()
}

#[get("/")]
fn index() -> &'static str {
    "
    USAGE

      POST /<id>

          accepts raw data in the body of the request and responds with a URL of
          a page containing the body's content

          EXAMPLE: curl --data-binary @file.txt http://localhost:8000/<id>

      GET /<id>

          retrieves the content for the paste with id `<id>`
    "
}


pub fn stage() -> AdHoc {
    AdHoc::on_ignite("Chunk Server Stage", |rocket| async {
        rocket.mount("/chunks", routes![index, upload, delete, retrieve])
    })
}
