#![cfg(feature = "native-cli")]

use std::{
    io::Read,
    sync::{Arc, LazyLock},
    thread,
};

use tokio::{
    runtime::Builder,
    sync::{
        mpsc::{self, error::TrySendError},
        watch,
    },
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct StdinHandle(u8);

impl StdinHandle {
    const NONE: StdinHandle = StdinHandle(u8::MAX);
}

pub struct StdinRouter {
    senders: Arc<tokio::sync::Mutex<Vec<mpsc::Sender<u8>>>>,
    target_tx: watch::Sender<StdinHandle>,
}

impl StdinRouter {
    pub fn global() -> &'static Self {
        static INSTANCE: LazyLock<StdinRouter> = LazyLock::new(|| {
            let senders = Arc::new(tokio::sync::Mutex::new(
                Vec::<mpsc::Sender<u8>>::with_capacity(4),
            ));

            let rt = Builder::new_current_thread().enable_all().build().unwrap();
            let (target_tx, mut target_rx) = watch::channel(StdinHandle::NONE);

            let senders_clone = Arc::clone(&senders);

            thread::spawn(move || {
                rt.block_on(async move {
                    loop {
                        let mut buf = [0u8; 1024];
                        let Ok(n) = std::io::stdin().read(&mut buf) else {
                            return;
                        };
                        if n == 0 {
                            return;
                        }
                        match forward_bytes(&mut target_rx, &senders_clone, &buf[..n]).await {
                            Err(ForwardError::Closed) => return,
                            _ => continue,
                        }
                    }
                });
            });

            StdinRouter { senders, target_tx }
        });

        &INSTANCE
    }

    pub fn register(&self, channel: mpsc::Sender<u8>) -> StdinHandle {
        let mut senders = self.senders.blocking_lock();

        if senders.len() == u8::MAX as usize {
            panic!("too many channels registered!")
        }

        senders.push(channel);
        StdinHandle((senders.len() - 1) as u8)
    }

    pub fn switch_to(&self, target: StdinHandle) {
        self.target_tx.send(target).unwrap();
    }
}

enum ForwardError {
    Closed,
    TargetChanged,
    TargetClosed,
}

async fn forward_bytes(
    target_rx: &mut watch::Receiver<StdinHandle>,
    senders: &tokio::sync::Mutex<Vec<tokio::sync::mpsc::Sender<u8>>>,
    buf: &[u8],
) -> Result<(), ForwardError> {
    let target = *target_rx.borrow_and_update();
    if target == StdinHandle::NONE {
        return Ok(());
    }
    let id = target.0 as usize;
    let sender = &mut senders.lock().await[id];
    for &b in buf {
        // use non-blocking send for performance
        match sender.try_send(b) {
            Ok(()) => {
                continue;
            }
            Err(TrySendError::Closed(_)) => {
                if let Err(_) = target_rx.changed().await {
                    return Err(ForwardError::Closed);
                }
                return Err(ForwardError::TargetClosed);
            }
            Err(TrySendError::Full(_)) => {
                // keep going
            }
        }

        tokio::select! {
            biased;  // poll in order

            _ = sender.send(b) => {}

            // discard all not send bytes (as intended)
            rst = target_rx.changed() => {
                match rst {
                    Ok(()) => { return Err(ForwardError::TargetChanged); }
                    Err(_) => { return Err(ForwardError::Closed); }
                }
            }
        }
    }

    Ok(())
}
