use anyhow::Result;
use async_std::task;
use futures::StreamExt;
use futures::stream::{select_all};
use std::sync::atomic::{AtomicU32, Ordering};
use zbus::Proxy;
use zbus::export::futures_core::Stream;
use zbus::fdo::DBusProxy;
use zbus::{Connection};
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

// Type alias for the stream of D-Bus messages
type DbusSignalStream = std::pin::Pin<Box<dyn Stream<Item = zbus::Message> + Send>>;

// Struct to indicate to the consumer which action to perform (addition/removal of a player)
type MediaPlayerListChangeSignal = ();

// Type alias for the producer channel
type ProducerChannel = async_std::channel::Sender<MediaPlayerListChangeSignal>;

// Type alias for the consumer channel
type ConsumerChannel = async_std::channel::Receiver<MediaPlayerListChangeSignal>;

fn main() -> Result<()> {
    // Handle the execution of main asynchronously
    task::block_on(async_main())
}

async fn async_main() -> Result<()> {
    // Open a channel for producer/consumer of monitored media players
    let (producer, consumer) = async_std::channel::unbounded::<MediaPlayerListChangeSignal>();

    // Monitor the additional/removal of media players from the D-Bus
    println!("[SYSTEM] Spawning task to monitor addition/removal of media players...");
    let monitor_players_task = task::spawn(async move { monitor_media_players(producer).await });

    // Monitor the playback status of active media players
    println!("[SYSTEM] Spawning task to monitor playback status of active media players...");
    let monitor_playback_task = task::spawn(async move { monitor_playback_status(consumer).await });

    // Wait for both tasks to finish
    let _ = futures::future::join_all(vec![monitor_players_task, monitor_playback_task]).await;
    Ok(())
}

async fn monitor_media_players(channel_notifier: ProducerChannel) -> Result<()> {
    // Establish a connection to the D-Bus interface in linux
    let conn = Connection::session().await?;

    // Create a proxy for the D-Bus interface
    let dbus: DBusProxy = DBusProxy::new(&conn).await?;

    // Receive all the signals matching the added rule
    let mut signal_stream = dbus.receive_name_owner_changed().await?;

    // Log that the service is monitoring for media player signals
    println!("[DISCOVERY] Media Player discovery service started");

    // Process the signals
    while let Some(signal) = signal_stream.next().await {
        // Deserialize the message arguments into the three known NameOwnerChanged strings.
        let (service_name, _old_owner, _new_owner) = match signal.args() {
            Ok(args) => (args.name, args.old_owner, args.new_owner),
            Err(e) => {
                eprintln!("[DISCOVERY] Failed to deserialize signal arguments: {}", e);
                continue;
            }
        };

        // If the name of the signal is not for a media player
        if !is_media_player(&service_name) {
            continue;
        }

        // Log that a change has been detected
        println!("[DISCOVERY] Detected change in list of media players");

        // Log the action that is taken for the changed list
        if _old_owner.is_none() && _new_owner.is_some() {
            println!("[DISCOVERY] {} has been added", service_name);
        } else if _old_owner.is_some() && _new_owner.is_none() {
            println!("[DISCOVERY] {} has been removed", service_name);
        }

        // Send a signal to Task 2 to rebuild its list of media players
        match channel_notifier.send(()).await {
            Ok(_) => {
                eprintln!("[DISCOVERY] Playback monitor has been notified of detected changes")
            },
            Err(e) => {
                eprintln!("[DISCOVERY] Failed to notify playback monitor of detected changes: {}", e)
            }
        }
    }

    Ok(())
}

async fn monitor_playback_status(mut channel_notifier: ConsumerChannel) -> Result<()> {
    // Establish a connection to the D-Bus interface in linux
    let conn = Connection::session().await?;

    // Define a mutable variable for if the screen is currently being blocked
    let mut is_screen_blocked = false;

    // Initialise a list of current media players
    let streams = get_media_player_streams(&conn).await?;

    // Select all the streams
    let mut unified_stream = select_all(streams);

    // Log that the service is monitoring for playback changes in media players
    println!("[PLAYBACK] Media Playback monitor service started");

    // Update the blocking to state to match the initial state
    update_blocking_state(&conn, &mut is_screen_blocked).await?;

    loop {
        // If the list of media player streams is empty
        if unified_stream.is_empty() {
            // If no players are found, wait ONLY for the next notification from Task 1
            println!("[PLAYBACK] No active media players found, waiting until nest media player notification...");

            // Wait for the list of media players to update
            let _ = channel_notifier.next().await;

            // Get an updated list of streams
            let new_streams = get_media_player_streams(&conn).await?;

            // Update the list of streams with the new list
            unified_stream = select_all(new_streams);

            // Update the blocking state to the current state
            update_blocking_state(&conn, &mut is_screen_blocked).await?;
            continue;
        }

        // Wait for the next signal, either for new media players or playback status change
        futures::select! {
            // If a new media player was added/removed from Task 1
            _ = channel_notifier.next() => {
                // If no players are found, wait ONLY for the next notification from Task 1
                println!("[PLAYBACK] Received discovery signal, refreshing list of media players...");

                // Get an updated list of streams
                let new_streams = get_media_player_streams(&conn).await?;

                // Update the list of streams with the new list
                unified_stream = select_all(new_streams);

                // Update the blocking state to the current state
                update_blocking_state(&conn, &mut is_screen_blocked).await?;
                continue;
            },

            // If the playback status changed for an active player
            result = unified_stream.next() => {
                // If no players are found, wait ONLY for the next notification from Task 1
                if result.is_some() {
                    update_blocking_state(&conn, &mut is_screen_blocked).await?;
                }
            },
        }
    }
}

async fn get_media_player_streams(conn: &Connection) -> Result<Vec<DbusSignalStream>> {
    // Get a list of all the media players
    let media_players = get_media_player_names(conn).await?;

    // Define a mutable list of streams for each of the players
    let mut streams = Vec::new();

    // Loop over all the media player's
    for player_name in media_players {
        // Create a proxy object for the playback properties of the media player
        let player = Proxy::new(
            conn,
            player_name.clone(),
            MEDIA_PLAYER_PATH,
            MEDIA_PLAYER_INTERFACE,
        )
        .await?;

        // Listen for any changes in the properties of the media player
        if let Ok(stream) = player.receive_signal("PropertiesChanged").await {
            streams.push(Box::pin(stream) as DbusSignalStream);
        } else {
            eprintln!("Failed to register signal for player: {}", player_name);
        }
    }

    Ok(streams)
}

async fn update_blocking_state(conn: &Connection, currently_blocking: &mut bool) -> Result<()> {
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

async fn block_screen(conn: &Connection) -> Result<()> {
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

async fn unblock_screen(conn: &Connection) -> Result<()> {
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

async fn any_playing_media(conn: &Connection) -> Result<bool> {
    // Get the names of the media players for the D-Bus session
    let media_players = get_media_player_names(&conn).await?;

    // For each of the media players
    for player_name in media_players {
        // Get and match on the playback status of the player
        match get_playback_status(&conn, &player_name).await {
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

async fn get_media_player_names(conn: &Connection) -> Result<Vec<String>> {
    // Wrap the D-Bus daemon in a proxy layer to interface with methods or properties
    let dbus = Proxy::new(&conn, DBUS_DESTINATION, DBUS_PATH, DBUS_INTERFACE).await?;

    // Get the names in the D-Bus
    let names: Vec<String> = dbus.call("ListNames", &()).await?;

    // Filter the names of the media players
    Ok(names
        .into_iter()
        .filter(|name| is_media_player(name))
        .collect())
}

async fn get_playback_status(conn: &Connection, player: &str) -> Result<Option<String>> {
    // Open a proxy layer to the D-Bus to interface with its methods or properties
    let properties = Proxy::new(&conn, player, MEDIA_PLAYER_PATH, MEDIA_PLAYER_INTERFACE).await?;

    // Get the playback status from the player
    let body = ("org.mpris.MediaPlayer2.Player", "PlaybackStatus");
    let status: Result<OwnedValue, _> = properties.call("Get", &body).await;

    // Check for the existence of the property
    match status {
        Ok(value) => Ok(Some(value.to_string())),
        Err(_) => Ok(None),
    }
}

fn is_media_player(name: &str) -> bool {
    static FILTER: &str = "org.mpris.MediaPlayer2";
    name.starts_with(FILTER)
}

fn is_playback_running(status: &str) -> bool {
    // Return if the status is running
    status.to_lowercase().contains("playing")
}
