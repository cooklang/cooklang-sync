use crate::chunk_id::ChunkId;

// todo try to avoid nesting?
#[derive(Debug)]
pub(crate) struct MultiChunkResponse<'a> {
    chunk_ids: Vec<ChunkId<'a>>,
}

impl MultiChunkResponse<'_> {
    pub fn new(chunk_ids: Vec<ChunkId<'_>>) -> MultiChunkResponse<'_> {
        MultiChunkResponse { chunk_ids }
    }
}

// impl<'r> Responder<'r, 'static> for MultiChunkResponse<'_> {
//     fn respond_to(self, _: &'r Request<'_>) -> rocket::response::Result<'static> {
//         let mut response = Response::build();
//         response.header(ContentType::new("multipart", "mixed; boundary=AXX-BOUNDARY"));

//     //     if id == EMPTY_CHUNK_ID {
//     //     None
//     // } else {
//     //     File::open(id.file_path()).await.map(RawText).ok()
//     // }

//         let mut body = Vec::new();
//         for chunk_id in self.chunk_ids {
//             body.extend_from_slice(format!("--{}\r\n", "AXX-BOUNDARY").as_bytes());
//             body.extend_from_slice(format!("Content-Type: {}\r\n", "application/octet-stream").as_bytes());
//             body.extend_from_slice(format!("Content-Disposition: attachment; filename=\"{}\"\r\n", chunk_id.id()).as_bytes());
//             body.extend_from_slice(b"\r\n");

//             let mut file = File::open(chunk_id.file_path()).await.expect("shitteee");
//             let mut contents = vec![];
//             file.read_to_end(&mut contents).await.expect("shitteee");

//             body.extend_from_slice(&contents);

//             body.extend_from_slice(b"\r\n");
//         }
//         body.extend_from_slice(format!("--{}--\r\n", "AXX-BOUNDARY").as_bytes());

//         response.sized_body(body.len(), Cursor::new(body));
//         response.ok()
//     }
// }
