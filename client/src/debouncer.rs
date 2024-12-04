use gloo_timers::callback::Timeout;
use std::time::Duration;

pub(crate) struct Debouncer {
    timeout: Option<Timeout>,
}

impl Debouncer {
    pub(crate) fn new() -> Self {
        Self { timeout: None }
    }

    pub(crate) fn debounce<F>(&mut self, delay: Duration, callback: F)
    where
        F: 'static + FnOnce(),
    {
        if let Some(timeout) = self.timeout.take() {
            timeout.cancel();
        }

        self.timeout = Some(Timeout::new(delay.as_millis() as u32, callback));
    }
}


