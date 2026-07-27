use std::pin::Pin;

use tokio::runtime::Builder;
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::TrySendError;

type TaskFuture = Pin<Box<dyn Future<Output = ()> + 'static + Send>>;

#[derive(Clone)]
pub struct TaskSpawner {
    spawn: mpsc::Sender<TaskFuture>,
}

impl TaskSpawner {
    pub fn new() -> TaskSpawner {
        let (send, mut recv) = mpsc::channel(64);

        let runtime = Builder::new_current_thread().enable_all().build().unwrap();

        std::thread::spawn(move || {
            runtime.block_on(async move {
                let mut tasks = Vec::new();
                while let Some(task) = recv.recv().await {
                    tasks.push(tokio::spawn(task));
                }

                for task in tasks {
                    let _ = task.await;
                }
            });
        });

        TaskSpawner { spawn: send }
    }

    pub fn spawn_task(&self, task: TaskFuture) {
        match self.spawn.try_send(task) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => {
                panic!("Too much tasks in queue.")
            }
            Err(TrySendError::Closed(_)) => {
                panic!("The shared runtime has shut down.");
            }
        }
    }
}
