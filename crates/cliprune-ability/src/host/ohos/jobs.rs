//! Worker-side Ability calls, following PhotoCraft's worker-safe bridge executor.

use std::{
    future::Future,
    sync::{Arc, mpsc},
    task::{Context, Poll, Wake, Waker},
};

pub fn block_on<T>(future: impl Future<Output = T>) -> T {
    struct Unpark(std::thread::Thread);
    impl Wake for Unpark {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    let waker = Waker::from(Arc::new(Unpark(std::thread::current())));
    let mut context = Context::from_waker(&waker);
    let mut future = std::pin::pin!(future);
    loop {
        if let Poll::Ready(value) = future.as_mut().poll(&mut context) {
            return value;
        }
        std::thread::park();
    }
}

/// A stopped worker closes the reply channel and reports its error to the editor.
pub fn spawn<T: Send + 'static>(
    name: &str,
    future: impl Future<Output = Result<T, String>> + Send + 'static,
    ctx: egui::Context,
    errors: mpsc::Sender<String>,
) -> mpsc::Receiver<T> {
    let (tx, rx) = mpsc::channel();
    let worker_ctx = ctx.clone();
    let worker_errors = errors.clone();
    let result = std::thread::Builder::new().name(format!("ohos-{name}")).spawn(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| block_on(future)))
            .map_err(|_| "OpenHarmony operation stopped unexpectedly; please retry".to_owned())
            .and_then(|result| result);
        match result {
            Ok(value) => {
                let _ = tx.send(value);
            }
            Err(error) => {
                log::error!("{error}");
                let _ = worker_errors.send(error);
            }
        }
        worker_ctx.request_repaint();
    });
    if let Err(error) = result {
        let error = format!("Could not start OpenHarmony {name}: {error}");
        log::error!("{error}");
        let _ = errors.send(error);
        ctx.request_repaint();
    }
    rx
}
