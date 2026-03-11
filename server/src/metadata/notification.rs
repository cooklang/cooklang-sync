use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_notify::Notify;

const MAX_POLL_SECONDS: u64 = 120;

pub(crate) struct ActiveClients {
    clients: HashMap<String, Arc<Notify>>,
}

pub(crate) fn init() -> Mutex<ActiveClients> {
    Mutex::new(ActiveClients {
        clients: HashMap::new(),
    })
}

impl ActiveClients {
    pub(crate) fn register(&mut self, uuid: &str) -> Arc<Notify> {
        self.clients
            .entry(uuid.to_string())
            .or_insert_with(|| Arc::new(Notify::new()))
            .clone()
    }

    pub(crate) fn remove(&mut self, uuid: &str) {
        self.clients.remove(uuid);
    }

    pub(crate) fn notify(&self, uuid: &str) {
        for (client_uuid, notification) in &self.clients {
            if client_uuid != uuid {
                notification.notify();
            }
        }
    }
}

pub(crate) fn clamp_poll_seconds(seconds: u64) -> u64 {
    seconds.min(MAX_POLL_SECONDS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_returns_notification() {
        let mutex = init();
        let mut clients = mutex.lock().unwrap();
        let notify = clients.register("abc");
        assert!(Arc::strong_count(&notify) == 2); // one in map, one returned
    }

    #[test]
    fn register_same_uuid_returns_same_notify() {
        let mutex = init();
        let mut clients = mutex.lock().unwrap();
        let n1 = clients.register("abc");
        let n2 = clients.register("abc");
        assert!(Arc::ptr_eq(&n1, &n2));
    }

    #[test]
    fn remove_cleans_up_client() {
        let mutex = init();
        let mut clients = mutex.lock().unwrap();
        clients.register("abc");
        assert_eq!(clients.clients.len(), 1);
        clients.remove("abc");
        assert_eq!(clients.clients.len(), 0);
    }

    #[test]
    fn notify_skips_sender() {
        let mutex = init();
        let mut clients = mutex.lock().unwrap();
        let _n1 = clients.register("sender");
        let _n2 = clients.register("receiver");
        // Should not panic; sender is excluded
        clients.notify("sender");
    }

    #[test]
    fn clamp_poll_seconds_caps_value() {
        assert_eq!(clamp_poll_seconds(200), MAX_POLL_SECONDS);
        assert_eq!(clamp_poll_seconds(60), 60);
        assert_eq!(clamp_poll_seconds(0), 0);
    }
}
