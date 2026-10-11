//! Blocking HTTP belongs on dedicated threads, never GPUI's executor workers.
pub async fn run<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    let (send, receive) = futures::channel::oneshot::channel();
    std::thread::spawn(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(work))
            .unwrap_or_else(|_| Err("The request failed unexpectedly. Please try again.".into()));
        let _ = send.send(result);
    });
    receive
        .await
        .map_err(|_| "Request worker disconnected".to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn completion_and_worker_failure_always_resolve() {
        assert_eq!(futures::executor::block_on(run(|| Ok(42))).unwrap(), 42);
        assert!(futures::executor::block_on(run::<()>(|| panic!("failed request"))).is_err());
    }
}
