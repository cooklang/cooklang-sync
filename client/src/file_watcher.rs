use futures::{
    channel::mpsc::{channel, Receiver},
    SinkExt,
};
use notify_debouncer_mini::{notify::*,new_debouncer,DebounceEventResult,Debouncer};
use std::time::Duration;


const CHANNEL_SIZE: usize = 1000;
const DEBOUNCE_SEC: u64 = 2;

pub fn async_watcher() -> notify::Result<(Debouncer<FsEventWatcher>, Receiver<DebounceEventResult>)> {
    let (mut tx, rx) = channel(CHANNEL_SIZE);

    let debouncer = new_debouncer(Duration::from_secs(DEBOUNCE_SEC), move |res: DebounceEventResult| {
        futures::executor::block_on(async {
            tx.send(res).await;
        })
    })?;

    Ok((debouncer, rx))
}
