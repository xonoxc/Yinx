use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::oneshot;
use yinx_core::request::Request;
use yinx_core::response::Response;
use crate::client::HttpClient;

#[derive(Debug)]
pub enum RequestEvent {
    Completed(Response, u64),
    Failed(String),
}

pub struct RequestController {
    cancel_flag: Arc<AtomicBool>,
    task_handle: Option<tokio::task::JoinHandle<()>>,
}

impl RequestController {
    pub fn new() -> Self {
        Self {
            cancel_flag: Arc::new(AtomicBool::new(false)),
            task_handle: None,
        }
    }

    pub fn execute(
        &mut self,
        request: Request,
        client: HttpClient,
    ) -> oneshot::Receiver<RequestEvent> {
        self.cancel();
        self.cancel_flag.store(false, Ordering::SeqCst);

        let cancel = self.cancel_flag.clone();
        let (tx, rx) = oneshot::channel();
        let started_at = std::time::Instant::now();

        let handle = tokio::spawn(async move {
            if cancel.load(Ordering::SeqCst) {
                let _ = tx.send(RequestEvent::Failed("cancelled".to_string()));
                return;
            }

            let elapsed_ms = started_at.elapsed().as_millis() as u64;
            match client.send_request(request).await {
                Ok(mut response) => {
                    response.timing_ms = elapsed_ms;
                    let _ = tx.send(RequestEvent::Completed(response, elapsed_ms));
                }
                Err(e) => {
                    let _ = tx.send(RequestEvent::Failed(e.to_string()));
                }
            }
        });

        self.task_handle = Some(handle);
        rx
    }

    pub fn cancel(&mut self) {
        self.cancel_flag.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel_flag.load(Ordering::SeqCst)
    }
}

impl Default for RequestController {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_controller_new() {
        let controller = RequestController::new();
        assert!(!controller.is_cancelled());
    }

    #[test]
    fn test_request_controller_cancel() {
        let mut controller = RequestController::new();
        controller.cancel();
        assert!(controller.is_cancelled());
    }

    #[test]
    fn test_request_controller_cancel_reset() {
        let mut controller = RequestController::new();
        controller.cancel();
        assert!(controller.is_cancelled());
        controller.cancel_flag.store(false, Ordering::SeqCst);
        assert!(!controller.is_cancelled());
    }
}
