

pub struct Remote {
    token: String
}

impl Remote {

    pub fn new(token: &str) -> Remote {
        Self {
            token: token.to_string()
        }
    }
}
// impl Remote {

//     fn upload(chunk, content) {

//     }

//     fn download(chunk, content) {

//     }

//     fn list(namespace, local_jid) {

//     }
// }
