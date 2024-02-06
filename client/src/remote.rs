use serde::{Deserialize, Serialize};

use bytes::Bytes;

type Result<T, E = reqwest::Error> = std::result::Result<T, E>;

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
    client: reqwest::Client,
}

impl Remote {

    pub fn new(api_endpoint: &str, token: &str) -> Remote {
        Self {
            api_endpoint: api_endpoint.into(),
            token: token.into(),
            client: reqwest::Client::new()
        }
    }
}
impl Remote {

    pub async fn upload(&self, chunk: &str, content: Bytes) -> Result<()>{
        self.client
            .post(self.api_endpoint.clone() + "/chunks/" + chunk)
            .body(content)
            .send()
            .await?;

        Ok(())
    }

    pub async fn download(&self, chunk: &str) -> Result<Bytes>{
        let response = self.client
            .get(self.api_endpoint.clone() + "/chunks/" + chunk)
            .send()
            .await?;

        response.bytes().await
    }

    pub async fn list(&self, local_jid: i32) -> Result<Vec<ResponseFileRecord>> {
        let jid_string = local_jid.to_string();

        let res = self.client
            .get(self.api_endpoint.clone() + "/metadata/list?jid=" + &jid_string)
            .send()
            .await?;

        res.json().await
    }

    pub async fn poll(&self, seconds: i32) -> Result<()> {
        let seconds_string = seconds.to_string();

        let res = self.client
            .get(self.api_endpoint.clone() + "/metadata/poll?seconds=" + &seconds_string)
            .send()
            .await?;

        res.json().await
    }

    pub async fn commit(&self, path: &str, deleted: bool, chunk_ids: &str, format: &str) -> Result<CommitResultStatus> {
        let params = [
            ("format", format),
            ("deleted", if deleted { "true" } else { "false" }),
            ("chunk_ids", chunk_ids),
            ("path", path),
        ];

        let res = self.client
            .post(self.api_endpoint.clone() + "/metadata/commit")
            .form(&params)
            .send()
            .await?;

        res.json().await
    }
}
