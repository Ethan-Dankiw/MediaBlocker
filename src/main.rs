use anyhow::Result;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;
use zbus::blocking::Connection;
use zbus::blocking::Proxy;
use zvariant::OwnedValue;

// Paths to DBus object
const DBUS_DESTINATION: &str = "org.freedesktop.DBus";
const DBUS_PATH: &str = "/org/freedesktop/DBus";
const DBUS_INTERFACE: &str = "org.freedesktop.DBus";

// Paths to the MediaPlayer object
const MEDIA_PLAYER_PATH: &str = "/org/mpris/MediaPlayer2";
const MEDIA_PLAYER_INTERFACE: &str = "org.freedesktop.DBus.Properties";

// Paths to the Idle Inhibition Service (ScreenSaver)
const SCREENSAVER_DESTINATION: &str = "org.freedesktop.ScreenSaver";
const SCREENSAVER_PATH: &str = "/org/freedesktop/ScreenSaver";
const SCREENSAVER_INTERFACE: &str = "org.freedesktop.ScreenSaver";

// Global atomic variable to store the inhibition cookie (0 means not inhibited)
static INHIBIT_COOKIE: AtomicU32 = AtomicU32::new(0);

fn main() -> Result<()> {
    // Establish a connection to the D-Bus interface in linux
    let conn = Connection::session()?;

    // Mutable boolean for if the screen is currently being blocked
    let mut is_screen_blocked = false;

    // Print the start message
    println!("Starting media blocker to monitor playback and block screen locking");
    println!("Ctrl + C to STOP");

    // Loop forever
    loop {
        // Check if any media is current playing
        match any_playing_media(&conn) {
            Ok(is_media_playing) => {
                // If media is playing, and the screen is not being blocked
                if is_media_playing && !is_screen_blocked {
                    // Block the screen
                    match block_screen(&conn) {
                        Ok(_) => {
                            // Set the screen to blocking
                            is_screen_blocked = true;
                            println!("Screen is now blocked")
                        }
                        Err(e) => eprintln!("Failed to block screen: {}", e),
                    }
                } else if !is_media_playing && is_screen_blocked {
                    // If media is not playing and the screen is currently being blocked
                    match unblock_screen(&conn) {
                        Ok(_) => {
                            // Set the screen to not blocked
                            is_screen_blocked = false;
                            println!("Screen is not longer being blocked")
                        }
                        Err(e) => eprintln!("Failed to unblock screen: {}", e),
                    }
                }
            }
            Err(e) => eprintln!("Error checking media playback status: {}", e),
        }

        // Wait for a few seconds before checking again (to prevent excessive D-Bus calls)
        std::thread::sleep(Duration::from_secs(3));
    }
}

fn block_screen(conn: &Connection) -> Result<()> {
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
    )?;

    // Define the application name and reason for blocking
    let app_name = "Rust Media Monitor".to_string();
    let reason = "Media is current playing".to_string();

    // Call the inhibit method to block the screen
    let cookie: u32 = screensaver.call("Inhibit", &(app_name, reason))?;

    // Store the cookie globally
    INHIBIT_COOKIE.store(cookie, Ordering::SeqCst);

    // Return that the screen is currently being blocked
    Ok(())
}

fn unblock_screen(conn: &Connection) -> Result<()> {
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
    )?;

    // Remove the inhibit cookie and unblock the screen
    screensaver.call::<&str, _, ()>("UnInhibit", &(cookie))?;

    // Return that the screen is no longer being blocked
    Ok(())
}

fn any_playing_media(conn: &Connection) -> Result<bool> {
    // Get the names of the media players for the D-Bus session
    let media_players = get_media_player_names(&conn)?;

    // For each of the media players
    for player_name in media_players {
        // Get and match on the playback status of the player
        match get_playback_status(&conn, &player_name) {
            Ok(Some(status)) => {
                // Check if the playback status indicates media is being played
                if is_playback_running(&status) {
                    return Ok(true);
                }
            }
            Ok(None) => {}
            Err(e) => {
                eprintln!("{} -> Error getting playback status: {}", player_name, e);
            }
        }
    }

    // If no match was found no player is running
    Ok(false)
}

fn get_media_player_names(conn: &Connection) -> Result<Vec<String>> {
    // Wrap the D-Bus daemon in a proxy layer to interface with methods or properties
    let dbus = Proxy::new(&conn, DBUS_DESTINATION, DBUS_PATH, DBUS_INTERFACE)?;

    // Get the names in the D-Bus
    let names: Vec<String> = dbus.call("ListNames", &())?;

    // Filter the names of the media players
    let filter = "org.mpris.MediaPlayer2";
    Ok(names
        .into_iter()
        .filter(|name| name.starts_with(filter))
        .collect())
}

fn get_playback_status(conn: &Connection, player: &str) -> Result<Option<String>> {
    // Open a proxy layer to the D-Bus to interface with its methods or properties
    let properties = Proxy::new(&conn, player, MEDIA_PLAYER_PATH, MEDIA_PLAYER_INTERFACE)?;

    // Get the playback status from the player
    let body = ("org.mpris.MediaPlayer2.Player", "PlaybackStatus");
    let status: Result<OwnedValue, _> = properties.call("Get", &body);

    // Check for the existence of the property
    match status {
        Ok(value) => Ok(Some(value.to_string())),
        Err(_) => Ok(None),
    }
}

fn is_playback_running(status: &str) -> bool {
    // Return if the status is running
    status.to_lowercase().contains("playing")
}
