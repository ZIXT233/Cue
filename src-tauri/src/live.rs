use tokio::sync::broadcast;

#[derive(Clone)]
pub struct LiveBus {
    tx: broadcast::Sender<String>,
}

impl LiveBus {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(64);
        Self { tx }
    }

    pub fn notify(&self, reason: &str) {
        let _ = self.tx.send(reason.to_string());
    }

    pub fn subscribe(&self) -> broadcast::Receiver<String> {
        self.tx.subscribe()
    }
}
