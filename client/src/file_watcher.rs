use futures::{
    channel::mpsc::{channel, Receiver},
    SinkExt,
};
use notify::RecommendedWatcher;
use notify_debouncer_mini::{new_debouncer, DebounceEventResult, Debouncer};
use std::time::Duration;

const CHANNEL_SIZE: usize = 1000;
const DEBOUNCE_SEC: u64 = 2;

pub fn async_watcher(
) -> notify::Result<(Debouncer<RecommendedWatcher>, Receiver<DebounceEventResult>)> {
    let (mut tx, rx) = channel(CHANNEL_SIZE);

    let debouncer = new_debouncer(
        Duration::from_secs(DEBOUNCE_SEC),
        move |res: DebounceEventResult| {
            futures::executor::block_on(async {
                let _ = tx.send(res).await;
            })
        },
    )?;

    Ok((debouncer, rx))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::thread;
    use std::time::Duration;
    use futures::StreamExt;

    #[test]
    fn test_async_watcher_debounces_file_event() {
        let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
        let watch_path = temp_dir.path().to_path_buf();

        // Initialize the debouncer and receiver
        let (mut debouncer, mut rx) = async_watcher().expect("Failed to create debouncer");

        // Start watching the temporary directory
        debouncer.watcher().watch(&watch_path, notify::RecursiveMode::NonRecursive).expect("failed to watch directory");

        // Simulate creating a new file in the watched directory
        let file_path = watch_path.join("test_file.txt");
        File::create(&file_path).expect("Failed to create file");

        // Wait longer than the debounce time 
        thread::sleep(Duration::from_secs(3));

        // Check if the debounced event was received via the receiver
        let mut received_event = false;
        while let Some(event) = futures::executor::block_on(rx.next()) {
            match event {
                Ok(events) => {
                    for e in events {
                        if e.path == *file_path{
                            received_event = true;
                            break;
                        }
                    }
                }
                Err(e) => panic!("Error in debounced event: {:?}", e),
            }

            if received_event{
                break;
            }

            
        }

        assert!(received_event, "No file event was received");
    }
}

