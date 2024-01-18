use std::sync::mpsc::{Sender};

use notify::{RecursiveMode, Watcher};
use notify_debouncer_full::{new_debouncer, DebounceEventResult};
use std::{path::Path, time::Duration};

pub struct FileWatcher {}

impl FileWatcher {
    pub fn run(&self, path: String, sender: Sender<DebounceEventResult>) -> notify::Result<()> {
        // Create a new debounced file watcher with a timeout of 2 seconds.
        // The tickrate will be selected automatically, as well as the underlying watch implementation.
        let mut debouncer = new_debouncer(Duration::from_secs(2), None, sender)?;
        let path = Path::new(&path);

        // Add a path to be watched. All files and directories at that path and
        // below will be monitored for changes.
        debouncer
            .watcher()
            .watch(path, RecursiveMode::Recursive)?;

        // Initialize the file id cache for the same path. This will allow the debouncer to stitch together move events,
        // even if the underlying watch implementation doesn't support it.
        // Without the cache and with some watch implementations,
        // you may receive `move from` and `move to` events instead of one `move both` event.
        debouncer
            .cache()
            .add_root(path, RecursiveMode::Recursive);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::prelude::*;
    use std::env;
    use std::path::PathBuf;

    #[test]
    fn test_file_watcher_creation() {


        assert_eq!(watcher.path, ".");
    }

    #[test]
    fn test_file_watcher_event() {
        let (tx, rx) = mpsc::channel();
        let watcher = FileWatcher::new(tx, String::from("."));

        // Simulate file system change
        thread::spawn(move || {
            watcher.run();
        });

        let mut path = env::current_dir().unwrap();
        path.push("test_file.txt");

        // Create a new file to trigger an event
        let mut file = File::create(&path).unwrap();
        writeln!(file, "Hello, world!").unwrap();

        // Expect an event
        match rx.recv_timeout(Duration::from_secs(20)) {
            Ok(event) => match event {
                DebouncedEvent::Create(ref p) if p == &path => (),
                _ => panic!("Unexpected event: {:?}", event),
            },
            Err(e) => panic!("Did not receive an event: {:?}", e),
        }

        // Cleanup
        std::fs::remove_file(path).unwrap();
    }
}
