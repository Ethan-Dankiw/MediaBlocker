use crate::app::media_player::any_playing_media;
use crate::global_constants::{SCREENSAVER_DESTINATION, SCREENSAVER_INTERFACE, SCREENSAVER_PATH};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use zbus::{Connection, Proxy};

pub struct ScreensaverState {
    /// Indicate if the screensaver can allow block/unblock updates
    allow_updates: Arc<AtomicBool>,

    /// Indicate if the screensaver is currently being blocked
    blocked: Arc<AtomicBool>,

    /// Unique ID for the inhibit entry stored by KDE for the blocked screensaver (0 if unblocked)
    inhibit_cookie: Arc<AtomicU32>,
}

impl ScreensaverState {
    pub fn new() -> Self {
        Self {
            allow_updates: Arc::new(AtomicBool::new(true)),
            blocked: Arc::new(AtomicBool::new(false)),
            inhibit_cookie: Arc::new(AtomicU32::new(0)),
        }
    }

    pub fn allow_updates(&self) {
        self.allow_updates.store(true, Ordering::Release);
    }

    pub fn disallow_updates(&self) {
        self.allow_updates.store(false, Ordering::Release);
    }

    pub fn are_updates_allowed(&self) -> bool {
        self.allow_updates.load(Ordering::SeqCst)
    }

    pub fn is_blocked(&self) -> bool {
        self.blocked.load(Ordering::SeqCst)
    }

    pub async fn update_state(&self, conn: &Connection) -> anyhow::Result<()> {
        // If the screensaver disallows updates
        if !self.are_updates_allowed() {
            // If the screensaver is currently blocked
            if self.is_blocked() {
                // Unblock the screensaver as the systems should provide updates
                self.unblock(conn).await?;
            }

            // Return early
            return Ok(());
        }

        // Check if any media is currently playing
        let is_media_playing = any_playing_media(conn).await?;

        // Check if the screensaver is currently being blocked
        let is_screensaver_blocked = self.is_blocked();

        // If media is playing, and the screensaver is not being blocked
        if is_media_playing && !is_screensaver_blocked {
            self.block(conn).await?;
            log::debug!("[SCREENSAVER] Now in the BLOCKED state");
            return Ok(());
        }

        // If media is not playing, and the screensaver is being blocked
        if !is_media_playing && is_screensaver_blocked {
            self.unblock(conn).await?;
            log::debug!("[SCREENSAVER] Now in the UNBLOCKED state");
            return Ok(());
        }

        Ok(())
    }

    async fn block(&self, conn: &Connection) -> anyhow::Result<()> {
        // Check if the inhibit cookie is set
        if self.inhibit_cookie.load(Ordering::SeqCst) != 0 {
            // Return that the screen is already being blocked
            return Ok(());
        }

        // Open a new proxy to the screensaver
        let screensaver = Proxy::new(
            conn,
            SCREENSAVER_DESTINATION,
            SCREENSAVER_PATH,
            SCREENSAVER_INTERFACE,
        )
        .await?;

        // Define the application name and reason for blocking
        let app_name = "Rust Media Monitor".to_string();
        let reason = "Media is currently playing".to_string();

        // Call the inhibit method to block the screen
        let cookie: u32 = screensaver.call("Inhibit", &(app_name, reason)).await?;

        // Store the cookie globally
        self.inhibit_cookie.store(cookie, Ordering::SeqCst);
        self.blocked.store(true, Ordering::SeqCst);

        // Return that the screen is currently being blocked
        Ok(())
    }

    async fn unblock(&self, conn: &Connection) -> anyhow::Result<()> {
        // Load the cookie, then clear its state
        let cookie = self.inhibit_cookie.swap(0, Ordering::SeqCst);

        // If the cookie's value is 0, the screen is not currently being blocked
        if cookie == 0 {
            // So, do nothing
            return Ok(());
        }

        // Since the cookie has a value here, it means the screen is currently being blocked
        let screensaver = Proxy::new(
            conn,
            SCREENSAVER_DESTINATION,
            SCREENSAVER_PATH,
            SCREENSAVER_INTERFACE,
        )
        .await?;

        // Remove the inhibit cookie and unblock the screen
        screensaver
            .call::<&str, _, ()>("UnInhibit", &(cookie))
            .await?;
        self.blocked.store(false, Ordering::SeqCst);

        // Return that the screen is no longer being blocked
        Ok(())
    }
}
