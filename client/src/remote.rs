use serde::{Deserialize, Serialize};
use uuid::Uuid;

use log::{trace};

use reqwest::{multipart};

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
    uuid: String,
    client: reqwest::Client,
}

impl Remote {

    pub fn new(api_endpoint: &str, token: &str) -> Remote {
        Self {
            api_endpoint: api_endpoint.into(),
            uuid: Uuid::new_v4().into(),
            token: token.into(),
            client: reqwest::Client::new()
        }
    }
}
impl Remote {

    pub async fn upload(&self, chunk: &str, content: Vec<u8>) -> Result<()>{
        trace!("uploading chunk {:?}", chunk);

        self.client
            .post(self.api_endpoint.clone() + "/chunks/" + chunk)
            .body(content)
            .send()
            .await?;

        Ok(())
    }

    pub async fn upload_batch(&self, chunks: Vec<(&str, Vec<u8>)>) -> Result<()> {
        trace!("uploading chunks {:?}", chunks);

        let mut form = multipart::Form::new();

        for (chunk, content) in chunks {
            form = form.part(String::from(chunk), multipart::Part::bytes(content));
        }

        self.client
            .post(self.api_endpoint.clone() + "/chunks/")
            .multipart(form)
            .send()
            .await?;

        Ok(())
    }

    pub async fn download(&self, chunk: &str) -> Result<Vec<u8>>{
        trace!("downloading chunk {:?}", chunk);

        let response = self.client
            .get(self.api_endpoint.clone() + "/chunks/" + chunk)
            .send()
            .await?;

        match response.bytes().await {
            Ok(bytes) => Ok(bytes.to_vec()),
            Err(e) => Err(e)
        }
    }

    pub async fn list(&self, local_jid: i32) -> Result<Vec<ResponseFileRecord>> {
        trace!("list after {:?}", local_jid);

        let jid_string = local_jid.to_string();

        let response = self.client
            .get(self.api_endpoint.clone() + "/metadata/list?jid=" + &jid_string)
            .send()
            .await?;

        response.json().await
    }

    pub async fn poll(&self, seconds: i32) -> Result<()> {
        trace!("started poll");

        let seconds_string = seconds.to_string();

        let response = self.client
            .get(self.api_endpoint.clone() + "/metadata/poll?seconds=" + &seconds_string + "&uuid=" + &self.uuid)
            .send()
            .await?;

        response.json().await
    }

    pub async fn commit(&self, path: &str, deleted: bool, chunk_ids: &str, format: &str) -> Result<CommitResultStatus> {
        trace!("commit {:?}", path);

        let params = [
            ("format", format),
            ("deleted", if deleted { "true" } else { "false" }),
            ("chunk_ids", chunk_ids),
            ("path", path),
        ];

        let response = self.client
            .post(self.api_endpoint.clone() + "/metadata/commit" + "?uuid=" + &self.uuid)
            .form(&params)
            .send()
            .await?;

        response.json().await
    }
}
