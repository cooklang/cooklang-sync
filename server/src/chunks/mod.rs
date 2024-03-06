use std::io::Cursor;

use multer::Multipart;
use rocket::data::{ByteUnit, Data};
use rocket::fairing::AdHoc;
use rocket::response::content::RawText;
use rocket::tokio::fs::{create_dir_all, File};
use rocket::tokio::io::AsyncWriteExt;
use tokio_util::io::ReaderStream;

use crate::auth::user::User;
use crate::chunk_id::ChunkId;

mod request;

use crate::chunks::request::RawContentType;

const TEXT_LIMIT: ByteUnit = ByteUnit::Kibibyte(64);
const EMPTY_CHUNK_ID: ChunkId = ChunkId(std::borrow::Cow::Borrowed(""));

#[post("/", format = "multipart/form-data", data = "<upload>")]
async fn upload_chunks(
    _user: User,
    content_type: RawContentType<'_>,
    upload: Data<'_>,
) -> std::io::Result<()> {
    let boundary = multer::parse_boundary(content_type.0).unwrap();
    let upload_stream = upload.open(TEXT_LIMIT);
    let mut multipart = Multipart::new(ReaderStream::new(upload_stream), boundary);

    // TODO prevent from DDOS
    while let Ok(Some(field)) = multipart.next_field().await {
        let field_name = field.name().unwrap();
        let chunk_id = ChunkId::from(field_name);

        if chunk_id == EMPTY_CHUNK_ID {
            continue;
        }

        let full_path = chunk_id.file_path();

        if let Some(parent) = full_path.parent() {
            create_dir_all(parent).await?;
        }

        let mut file = File::create(full_path).await?;
        let bytes = field.bytes().await.unwrap();
        let mut cursor = Cursor::new(bytes);
        file.write_all_buf(&mut cursor).await.unwrap();
    }

    Ok(())
}

/// Downloads chunk from a storage
// TODO batch download
#[get("/<id>")]
async fn retrieve(_user: User, id: ChunkId<'_>) -> Option<RawText<File>> {
    if id == EMPTY_CHUNK_ID {
        None
    } else {
        File::open(id.file_path()).await.map(RawText).ok()
    }
}

pub fn stage() -> AdHoc {
    AdHoc::on_ignite("Chunk Server Stage", |rocket| async {
        rocket.mount("/chunks", routes![upload_chunks, retrieve])
    })
}
