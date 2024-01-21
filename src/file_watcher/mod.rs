use futures::{
    channel::mpsc::{channel, Receiver},
    SinkExt, StreamExt,
};
use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::Path;


pub fn async_watcher() -> notify::Result<(RecommendedWatcher, Receiver<notify::Result<Event>>)> {
    let (mut tx, rx) = channel(1);

    // Automatically select the best implementation for your platform.
    // You can also access each implementation directly e.g. INotifyWatcher.
    let watcher = RecommendedWatcher::new(
        move |res| {
            futures::executor::block_on(async {
                tx.send(res).await.unwrap();
            })
        },
        Config::default(),
    )?;

    Ok((watcher, rx))
}

async fn async_watch<P: AsRef<Path>>(path: P) -> notify::Result<()> {
    let (mut watcher, mut rx) = async_watcher()?;

    // Add a path to be watched. All files and directories at that path and
    // below will be monitored for changes.
    watcher.watch(path.as_ref(), RecursiveMode::Recursive)?;

    while let Some(res) = rx.next().await {
        match res {
            Ok(event) => println!("changed: {:?}", event),
            Err(e) => println!("watch error: {:?}", e),
        }
    }

    Ok(())
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
