use serde::{Deserialize, Serialize};
use uuid::Uuid;

use log::trace;

use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use reqwest::{multipart, StatusCode};
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};

use futures::{Stream, StreamExt};

use crate::errors::SyncError;
type Result<T, E = SyncError> = std::result::Result<T, E>;

pub const REQUEST_TIMEOUT_SECS: u64 = 60;

#[derive(Deserialize, Serialize, Debug)]
pub struct ResponseFileRecord {
    pub id: i32,
    pub path: String,
    pub deleted: bool,
    pub chunk_ids: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub enum CommitResultStatus {
    Success(i32),
    NeedChunks(String),
}

pub struct Remote {
    api_endpoint: String,
    token: String,
    uuid: String,
    client: ClientWithMiddleware,
}

impl Remote {
    pub fn new(api_endpoint: &str, token: &str) -> Remote {
        let rc = reqwest::ClientBuilder::new()
            .gzip(true)
            .timeout(std::time::Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .build()
            .unwrap();
        let client = ClientBuilder::new(rc)
            // .with(OriginalHeadersMiddleware)
            .build();

        Self {
            api_endpoint: api_endpoint.into(),
            uuid: Uuid::new_v4().into(),
            token: token.into(),
            client,
        }
    }
}
impl Remote {
    fn auth_headers(&self) -> HeaderMap {
        let auth_value = format!("Bearer {}", self.token);

        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, HeaderValue::from_str(&auth_value).unwrap());

        headers
    }

    pub async fn upload(&self, chunk: &str, content: Vec<u8>) -> Result<()> {
        trace!("uploading chunk {:?}", chunk);

        let response = self
            .client
            .post(self.api_endpoint.clone() + "/chunks/" + chunk)
            .headers(self.auth_headers())
            .body(content)
            .send()
            .await?;

        match response.status() {
            StatusCode::OK => Ok(()),
            StatusCode::UNAUTHORIZED => Err(SyncError::Unauthorized),
            _ => Err(SyncError::Unknown),
        }
    }

    pub async fn upload_batch(&self, chunks: Vec<(String, Vec<u8>)>) -> Result<()> {
        trace!(
            "uploading chunks {:?}",
            chunks
                .clone()
                .into_iter()
                .map(|(c, _)| c)
                .collect::<Vec<String>>()
        );

        // TODO make proper streaming
        let mut form = multipart::Form::new();

        for (chunk, content) in chunks {
            form = form.part(chunk, multipart::Part::bytes(content));
        }

        let response = self
            .client
            .post(self.api_endpoint.clone() + "/chunks/upload")
            .headers(self.auth_headers())
            .multipart(form)
            .send()
            .await?;

        match response.status() {
            StatusCode::OK => Ok(()),
            StatusCode::UNAUTHORIZED => Err(SyncError::Unauthorized),
            // TODO Don't need to error as it's sometimes fails??
            _ => Err(SyncError::Unknown),
        }
    }

    pub async fn download(&self, chunk: &str) -> Result<Vec<u8>> {
        trace!("downloading chunk {:?}", chunk);

        let response = self
            .client
            .get(self.api_endpoint.clone() + "/chunks/" + chunk)
            .headers(self.auth_headers())
            .send()
            .await?;

        match response.status() {
            StatusCode::OK => match response.bytes().await {
                Ok(bytes) => Ok(bytes.to_vec()),
                Err(_) => Err(SyncError::BodyExtractError),
            },
            StatusCode::UNAUTHORIZED => Err(SyncError::Unauthorized),
            _ => Err(SyncError::Unknown),
        }
    }

    pub async fn list(&self, local_jid: i32) -> Result<Vec<ResponseFileRecord>> {
        trace!("list after {:?}", local_jid);

        let jid_string = local_jid.to_string();

        let response = self
            .client
            .get(self.api_endpoint.clone() + "/metadata/list?jid=" + &jid_string)
            .headers(self.auth_headers())
            .send()
            .await?;

        match response.status() {
            StatusCode::OK => {
                let records = response.json::<Vec<ResponseFileRecord>>().await?;

                Ok(records)
            }
            StatusCode::UNAUTHORIZED => Err(SyncError::Unauthorized),
            _ => Err(SyncError::Unknown),
        }
    }

    pub async fn poll(&self, seconds: u64) -> Result<()> {
        trace!("started poll");

        let seconds_string = seconds.to_string();

        let response = self
            .client
            .get(
                self.api_endpoint.clone()
                    + "/metadata/poll?seconds="
                    + &seconds_string
                    + "&uuid="
                    + &self.uuid,
            )
            .headers(self.auth_headers())
            .send()
            .await;

        // Handle the response, ignoring timeout errors
        match response {
            Ok(response) => match response.status() {
                StatusCode::OK => Ok(()),
                StatusCode::UNAUTHORIZED => Err(SyncError::Unauthorized),
                // Don't need to error as it's expected to be cancelled from time to time
                _ => Ok(()),
            },
            Err(e) if e.is_timeout() => Ok(()), // Ignore timeout errors
            Err(e) => Err(e.into()),
        }
    }

    pub async fn commit(
        &self,
        path: &str,
        deleted: bool,
        chunk_ids: &str,
    ) -> Result<CommitResultStatus> {
        trace!("commit {:?}", path);

        let params = [
            ("deleted", if deleted { "true" } else { "false" }),
            ("chunk_ids", chunk_ids),
            ("path", path),
        ];

        let response = self
            .client
            .post(self.api_endpoint.clone() + "/metadata/commit" + "?uuid=" + &self.uuid)
            .headers(self.auth_headers())
            .form(&params)
            .send()
            .await?;

        match response.status() {
            StatusCode::OK => {
                let records = response.json::<CommitResultStatus>().await?;

                Ok(records)
            }
            StatusCode::UNAUTHORIZED => Err(SyncError::Unauthorized),
            _ => Err(SyncError::Unknown),
        }
    }

    pub async fn download_batch<'a>(
        &'a self,
        chunk_ids: Vec<&'a str>,
    ) -> impl Stream<Item = Result<(String, Vec<u8>)>> + Unpin + 'a {
        Box::pin(async_stream::try_stream! {
            trace!("Starting download_batch with chunk_ids: {:?}", chunk_ids);

            let params: Vec<(&str, &str)> = chunk_ids.iter().map(|&id| ("chunk_ids[]", id)).collect();

            let response = self
                .client
                .post(self.api_endpoint.clone() + "/chunks/download")
                .headers(self.auth_headers())
                .form(&params)
                .send()
                .await?;
            trace!("Received response with status: {:?}", response.status());

            match response.status() {
                StatusCode::OK => {
                    let content_type = response
                        .headers()
                        .get("content-type")
                        .and_then(|v| v.to_str().ok())
                        .ok_or(SyncError::BatchDownloadError(
                            "No content-type header".to_string(),
                        ))?
                        .to_string();

                    let boundary = content_type
                        .split("boundary=")
                        .nth(1)
                        .ok_or(SyncError::BatchDownloadError(
                            "No boundary in content-type header".to_string(),
                        ))?;

                    let boundary_bytes = format!("--{}", boundary).into_bytes();

                    let mut stream = response.bytes_stream();
                    let mut buffer = Vec::new();

                    while let Some(chunk) = stream.next().await {
                        let chunk = chunk?;
                        buffer.extend_from_slice(&chunk);

                        // Process complete parts from buffer
                        while let Some((part, remaining)) = extract_next_part(&buffer, &boundary_bytes)? {
                            if let Some((chunk_id, content)) = process_part(&part)? {
                                yield (chunk_id, content);
                            }
                            buffer = remaining;
                        }
                    }
                }
                StatusCode::UNAUTHORIZED => Err(SyncError::Unauthorized)?,
                _ => Err(SyncError::Unknown)?,
            }
        })
    }
}

// Helper function to extract the next complete part from the buffer
fn extract_next_part(buffer: &[u8], boundary: &[u8]) -> Result<Option<(Vec<u8>, Vec<u8>)>> {
    if let Some(start) = find_boundary(buffer, boundary) {
        if let Some(next_boundary) = find_boundary(&buffer[start + boundary.len()..], boundary) {
            let part = buffer[start + boundary.len()..start + boundary.len() + next_boundary].to_vec();
            let remaining = buffer[start + boundary.len() + next_boundary..].to_vec();
            Ok(Some((part, remaining)))
        } else {
            Ok(None) // Need more data
        }
    } else {
        Ok(None) // Need more data
    }
}

// Helper function to process a single part
fn process_part(part: &[u8]) -> Result<Option<(String, Vec<u8>)>> {
    if let Some(headers_end) = find_double_crlf(part) {
        let headers = std::str::from_utf8(&part[..headers_end])
            .map_err(|_| SyncError::BatchDownloadError("Invalid headers".to_string()))?;

        let chunk_id = headers
            .lines()
            .find(|line| line.starts_with("X-Chunk-ID:"))
            .and_then(|line| line.split(": ").nth(1))
            .ok_or(SyncError::BatchDownloadError("No chunk ID found".to_string()))?
            .trim()
            .to_string();

        // remove last 2 bytes as they are the boundary
        let content = part[headers_end + 4..part.len() - 2].to_vec();
        Ok(Some((chunk_id, content)))
    } else {
        Ok(None)
    }
}

// Helper function to find boundary in buffer
fn find_boundary(data: &[u8], boundary: &[u8]) -> Option<usize> {
    data.windows(boundary.len()).position(|window| window == boundary)
}

// Helper function to find double CRLF
fn find_double_crlf(data: &[u8]) -> Option<usize> {
    data.windows(4).position(|window| window == b"\r\n\r\n")
}
