use async_notify::Notify;
use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};

pub(crate) struct Client {
    uuid: String,
    pub(crate) notification: Arc<Notify>,
}

impl Client {
    pub(crate) fn new(uuid: String) -> Self {
        Client {
            uuid,
            notification: Arc::new(Notify::new()),
        }
    }
}
impl PartialEq for Client {
    fn eq(&self, other: &Self) -> bool {
        self.uuid == other.uuid
    }
}

impl Eq for Client {}

impl Hash for Client {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.uuid.hash(state);
    }
}

pub(crate) struct ActiveClients {
    pub(crate) clients: HashSet<Client>,
}

pub(crate) fn init() -> Mutex<ActiveClients> {
    Mutex::new(ActiveClients {
        clients: HashSet::new(),
    })
}

impl ActiveClients {
    pub(crate) fn notify(&self, uuid: String) {
        let notifications: Vec<Arc<Notify>> = self
            .clients
            .iter()
            .filter(|client| client.uuid != uuid)
            .map(|client| Arc::clone(&client.notification))
            .collect();

        for notification in notifications {
            notification.notify();
        }
    }
}
