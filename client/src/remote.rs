use serde::{Deserialize, Serialize};
use uuid::Uuid;

use log::trace;

use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use reqwest::{multipart, StatusCode};

use crate::errors::SyncError;

type Result<T, E = SyncError> = std::result::Result<T, E>;

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct ResponseFileRecord {
    pub id: i32,
    pub path: String,
    pub deleted: bool,
    pub chunk_ids: String,
    pub format: String,
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
    client: reqwest::Client,
}

impl Remote {
    pub fn new(api_endpoint: &str, token: &str) -> Remote {
        Self {
            api_endpoint: api_endpoint.into(),
            uuid: Uuid::new_v4().into(),
            token: token.into(),
            client: reqwest::Client::new(),
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
        trace!("uploading chunks {:?}", chunks);

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
            _ => {
                // Don't need to error as it's expected to be cancelled from time to time
                Ok(())
            }
        }
    }

    pub async fn commit(
        &self,
        path: &str,
        deleted: bool,
        chunk_ids: &str,
        format: &str,
    ) -> Result<CommitResultStatus> {
        trace!("commit {:?}", path);

        let params = [
            ("format", format),
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
}
