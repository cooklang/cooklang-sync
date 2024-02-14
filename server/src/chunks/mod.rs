use std::io;
use std::io::Cursor;

use crate::chunk_id::ChunkId;
use multer::Multipart;
use rocket::data::{ByteUnit};
use rocket::data::{Data};
use rocket::fairing::AdHoc;

use rocket::response::content::RawText;
use rocket::tokio::fs::{File, create_dir_all};
use tokio::io::AsyncWriteExt;



const TEXT_LIMIT: ByteUnit = ByteUnit::Kibibyte(64);

use rocket::request::{FromRequest, Outcome};

pub struct RawContentType<'r>(pub &'r str);

#[rocket::async_trait]
impl<'r> FromRequest<'r> for RawContentType<'r> {
    type Error = ();

    async fn from_request(req: &'r rocket::Request<'_>) -> Outcome<Self, Self::Error> {
        let header = req.headers().get_one("Content-Type").or(Some("")).unwrap();
        Outcome::Success(RawContentType(header))
    }
}

#[post("/", format = "multipart/form-data", data = "<upload>")]
async fn upload_chunks(content_type: RawContentType<'_>, upload: Data<'_>   ) -> io::Result<()> {
    let boundary = multer::parse_boundary(content_type.0).unwrap();
    let upload_stream = upload.open(TEXT_LIMIT);
    let mut multipart = Multipart::new(tokio_util::io::ReaderStream::new(upload_stream), boundary);

    // TODO prevent from DDOS
    while let Ok(Some(field)) = multipart.next_field().await {
        let field_name = field.name().unwrap();

        let chunk_id = ChunkId::from(field_name);
        let full_path = chunk_id.file_path();
        if let Some(parent) = full_path.parent() {
            create_dir_all(parent).await?;
        }
        let mut file = tokio::fs::File::create(full_path).await.unwrap();
        let bytes = field.bytes().await.unwrap();
        let mut cursor = Cursor::new(bytes);
        file.write_all_buf(&mut cursor).await.unwrap();
    }

    Ok(())
}

/// Downloads chunk from a storage
#[get("/<id>")]
async fn retrieve(id: ChunkId<'_>) -> Option<RawText<File>> {
    File::open(id.file_path()).await.map(RawText).ok()
}

pub fn stage() -> AdHoc {
    AdHoc::on_ignite("Chunk Server Stage", |rocket| async {
        rocket.mount("/chunks", routes![upload_chunks, retrieve])
    })
}
