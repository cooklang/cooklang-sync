use serde::{Deserialize, Serialize};
use uuid::Uuid;

use log::trace;

use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use reqwest::{multipart, StatusCode};
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};

use crate::errors::SyncError;
type Result<T, E = SyncError> = std::result::Result<T, E>;

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
        let rc = reqwest::ClientBuilder::new().gzip(true).build().unwrap();
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
            .post(self.api_endpoint.clone() + "/chunks/")
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

    pub async fn poll(&self, seconds: i32) -> Result<()> {
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
            .await?;

        match response.status() {
            StatusCode::OK => Ok(()),
            StatusCode::UNAUTHORIZED => Err(SyncError::Unauthorized),
            // Don't need to error as it's expected to be cancelled from time to time
            _ => Ok(()),
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

    pub async fn download_batch(&self, chunk_ids: Vec<&str>) -> Result<Vec<(String, Vec<u8>)>> {
        trace!("Starting download_batch with chunk_ids: {:?}", chunk_ids);

        let params: Vec<(&str, &str)> = chunk_ids.iter().map(|&id| ("chunk_ids[]", id)).collect();
        trace!("Constructed params for request: {:?}", params);

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
                    ))?;
                trace!("Content-Type of response: {:?}", content_type);

                let boundary =
                    content_type
                        .split("boundary=")
                        .nth(1)
                        .ok_or(SyncError::BatchDownloadError(
                            "No boundary in content-type header".to_string(),
                        ))?;
                trace!("Extracted boundary: {:?}", boundary);

                let boundary_string = format!("--{}", boundary);

                let bytes = response.bytes().await?;
                trace!("Received bytes: {:?}", bytes.len());

                let mut parts = Vec::new();
                let mut pos = 0;

                while pos < bytes.len() {
                    // Find next boundary
                    trace!("Searching for boundary at position: {:?}", pos);
                    let boundary_pos = bytes[pos..]
                        .windows(boundary_string.len())
                        .position(|window| window == boundary_string.as_bytes())
                        .map(|p| p + pos)
                        .ok_or(SyncError::BatchDownloadError(
                            "No boundary found".to_string(),
                        ))?;

                    // Skip initial boundary
                    if pos == 0 {
                        trace!("Skipping initial boundary");
                        pos = boundary_pos + boundary_string.len();
                        continue;
                    }

                    let part = &bytes[pos..boundary_pos];
                    if !part.is_empty() {
                        trace!("Processing part with length: {:?}", part.len());
                        // Split headers and content at first double CRLF
                        if let Some(headers_end) = find_double_crlf(part) {
                            let headers =
                                std::str::from_utf8(&part[..headers_end]).map_err(|_| {
                                    SyncError::BatchDownloadError("Invalid headers".to_string())
                                })?;

                            // Extract chunk ID from headers
                            let chunk_id = headers
                                .lines()
                                .find(|line| line.starts_with("X-Chunk-ID:"))
                                .and_then(|line| line.split(": ").nth(1))
                                .ok_or(SyncError::BatchDownloadError(
                                    "No chunk ID found".to_string(),
                                ))?
                                .trim()
                                .to_string();

                            // Get content (skipping the double CRLF)
                            let content = part[headers_end + 4..].to_vec();
                            trace!(
                                "Extracted chunk_id: {:?}, content length: {}",
                                chunk_id,
                                content.len()
                            );

                            parts.push((chunk_id, content));
                        }
                    }

                    pos = boundary_pos + boundary_string.len();

                    // Check if this is the final boundary
                    if pos + 4 <= bytes.len() && &bytes[pos..pos + 4] == b"--\r\n" {
                        break;
                    }
                }

                // Helper function to find double CRLF
                fn find_double_crlf(data: &[u8]) -> Option<usize> {
                    data.windows(4).position(|window| window == b"\r\n\r\n")
                }

                Ok(parts)
            }
            StatusCode::UNAUTHORIZED => {
                trace!("Unauthorized access during download_batch");
                Err(SyncError::Unauthorized)
            }
            _ => {
                trace!("Unknown error occurred during download_batch");
                Err(SyncError::Unknown)
            }
        }
    }
}
