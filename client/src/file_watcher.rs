use futures::{
    channel::mpsc::{channel, Receiver},
    SinkExt,
};
use notify::{Config, Event, RecommendedWatcher, Watcher};

const CHANNEL_SIZE: usize = 1000;

pub fn async_watcher() -> notify::Result<(RecommendedWatcher, Receiver<notify::Result<Event>>)> {
    let (mut tx, rx) = channel(CHANNEL_SIZE);

    // Automatically select the best implementation for your platform.
    // You can also access each implementation directly e.g. INotifyWatcher.
    let watcher = RecommendedWatcher::new(
        move |res| {
            futures::executor::block_on(async {
                tx.send(res).await;
            })
        },
        Config::default(),
    )?;

    Ok((watcher, rx))
}
