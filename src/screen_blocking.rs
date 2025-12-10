use crate::global_constants::{SCREENSAVER_DESTINATION, SCREENSAVER_INTERFACE, SCREENSAVER_PATH};
use crate::media_player::any_playing_media;
use std::sync::atomic::{AtomicU32, Ordering};
use zbus::{Connection, Proxy};

// Global atomic variable to store the inhibition cookie (0 means not inhibited)
static INHIBIT_COOKIE: AtomicU32 = AtomicU32::new(0);

pub async fn update_blocking_state(
    conn: &Connection,
    currently_blocking: &mut bool,
) -> anyhow::Result<()> {
    // Check if any media is current playing
    let is_media_playing = any_playing_media(conn).await?;

    // If there is media currently playing, and screen is not currently blocked
    if is_media_playing && !*currently_blocking {
        // Block the screen
        block_screen(conn).await?;
        *currently_blocking = true;
        println!("State Change: Auto-sleep is now NOT ACTIVE");
        return Ok(());
    }

    // If there is no media playing, and the screen is currently being blocked
    if !is_media_playing && *currently_blocking {
        // Unblock the screen
        unblock_screen(conn).await?;
        *currently_blocking = false;
        println!("State Change: Auto-sleep is now ACTIVE");
        return Ok(());
    }

    Ok(())
}

pub async fn block_screen(conn: &Connection) -> anyhow::Result<()> {
    // Check if the inhibit cookie is set
    if INHIBIT_COOKIE.load(Ordering::SeqCst) != 0 {
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
    INHIBIT_COOKIE.store(cookie, Ordering::SeqCst);

    // Return that the screen is currently being blocked
    Ok(())
}

pub async fn unblock_screen(conn: &Connection) -> anyhow::Result<()> {
    // Load the cookie, then clear its state
    let cookie = INHIBIT_COOKIE.swap(0, Ordering::SeqCst);

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

    // Return that the screen is no longer being blocked
    Ok(())
}
