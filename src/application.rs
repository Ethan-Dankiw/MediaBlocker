use std::sync::atomic::AtomicBool;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    // Master flag for if the application allows for the screensaver state to be changed
    pub allow_screensaver_updates: Arc<AtomicBool>,

    // Status indicator for is the application is current blocking the screensaver
    pub screensaver_blocked: Arc<AtomicBool>,
}

// Implement the application
impl AppState {
    /// Create a new instance of the Application
    ///
    /// #### Default behaviour
    ///
    /// * Allows for status changes in blocking/unblocking the screensaver
    /// * Does not block screensaver on startup
    pub fn new() -> Self {
        Self {
            allow_screensaver_updates: Arc::new(AtomicBool::new(true)),
            screensaver_blocked: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Allows for the application to update the blocking/unblocking state of the screensaver
    pub fn allow_screensaver_updates(&self) {
        self.allow_screensaver_updates.store(true, std::sync::atomic::Ordering::SeqCst);
    }

    /// Disallow for the application to update the blocking/unblocking state of the screensaver
    pub fn disallow_screensaver_updates(&self) {
        self.allow_screensaver_updates.store(false, std::sync::atomic::Ordering::SeqCst);
    }

    /// Check if the application can update the blocking/unblocking state of the screensaver
    pub fn can_screensaver_be_updated(&self) -> bool {
        self.allow_screensaver_updates.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Set the state of the screensaver to blocked
    pub fn block_screensaver(&self) {
        self.screensaver_blocked.store(true, std::sync::atomic::Ordering::SeqCst);
    }

    /// Set the state of the screensaver to be unblocked
    pub fn unblock_screensaver(&self) {
        self.screensaver_blocked.store(false, std::sync::atomic::Ordering::SeqCst);
    }

    /// Check the blocked/unblocked state of the screensaver
    pub fn is_screensaver_blocked(&self) -> bool {
        self.screensaver_blocked.load(std::sync::atomic::Ordering::SeqCst)
    }
}