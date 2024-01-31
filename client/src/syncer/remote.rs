use serde::{Deserialize, Serialize};

type Result<T, E = reqwest::Error> = std::result::Result<T, E>;

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct ResponseFileRecord {
    pub id: i32,
    pub path: String,
    pub chunk_ids: String,
    pub format: String,
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

    fn upload(&self, chunk: String, content: String) {

    }

    fn download(&self, chunk: String, content: String) {

    }

    pub async fn list(&self, local_jid: i32) -> Result<Vec<ResponseFileRecord>> {
        let jid_string = local_jid.to_string();

        let res = self.client
            .get(self.api_endpoint.clone() + "metadata" + "?jid=" + &jid_string)
            .send()
            .await?;

        res.json().await
    }
}
