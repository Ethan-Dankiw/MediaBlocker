mod global_constants;
mod media_player;
mod screen_blocking;
mod utils;
mod application;

use crate::global_constants::{
    ConsumerChannel, MediaPlayerListChangeSignal, ProducerChannel, UnifiedStream,
};
use crate::media_player::get_media_player_streams;
use crate::screen_blocking::update_blocking_state;
use crate::utils::is_media_player;
use anyhow::Result;
use async_std::task;
use futures::stream::select_all;
use futures::StreamExt;
use zbus::fdo::DBusProxy;
use zbus::Connection;

fn main() -> Result<()> {
    // Handle the execution of main asynchronously
    task::block_on(async_main())
}

async fn async_main() -> Result<()> {
    // Establish a connection to the D-Bus interface in linux
    let conn = Connection::session().await?;

    // Open a channel for producer/consumer of monitored media players
    let (producer, consumer) = async_std::channel::unbounded::<MediaPlayerListChangeSignal>();

    // Monitor the additional/removal of media players from the D-Bus
    println!("[SYSTEM] Spawning task to monitor addition/removal of media players...");
    let monitor_players_task = task::spawn(monitor_media_players(conn.clone(), producer));

    // Monitor the playback status of active media players
    println!("[SYSTEM] Spawning task to monitor playback status of active media players...");
    let monitor_playback_task = task::spawn(monitor_playback_status(conn.clone(), consumer));

    // Wait for both tasks to finish
    futures::future::join_all(vec![monitor_players_task, monitor_playback_task]).await;
    Ok(())
}

async fn monitor_media_players(conn: Connection, channel_notifier: ProducerChannel) -> Result<()> {
    // Create a proxy for the D-Bus interface
    let dbus: DBusProxy = DBusProxy::new(&conn).await?;

    // Receive all the signals matching the added rule
    let mut signal_stream = dbus.receive_name_owner_changed().await?;

    // Log that the service is monitoring for media player signals
    println!("[DISCOVERY] Media Player discovery service started");

    // Process the signals
    while let Some(signal) = signal_stream.next().await {
        // Deserialize the message arguments into the three known NameOwnerChanged strings.
        let args = match signal.args() {
            Ok(a) => a,
            Err(e) => {
                eprintln!("[DISCOVERY] Failed to deserialize signal arguments: {}", e);
                continue;
            }
        };

        // If the name of the signal is not for a media player
        let service_name = args.name;
        if !is_media_player(&service_name) {
            // Ignore non-media services
            continue;
        }

        // Log that a change has been detected
        println!("[DISCOVERY] Detected change in list of media players");

        // Extract the old and new owners of the service
        let old_owner = args.old_owner;
        let new_owner = args.new_owner;

        // Log the action that is taken for the changed list
        if old_owner.is_none() && new_owner.is_some() {
            println!("[DISCOVERY] {} has been added", service_name);
        } else if old_owner.is_some() && new_owner.is_none() {
            println!("[DISCOVERY] {} has been removed", service_name);
        }

        // Send a signal to Task 2 to rebuild its list of media players
        match channel_notifier.send(()).await {
            Ok(_) => {
                eprintln!("[DISCOVERY] Playback monitor has been notified of detected changes")
            }
            Err(e) => {
                eprintln!(
                    "[DISCOVERY] Failed to notify playback monitor of detected changes: {}",
                    e
                );
                break;
            }
        }
    }

    Ok(())
}

async fn monitor_playback_status(
    conn: Connection,
    mut channel_notifier: ConsumerChannel,
) -> Result<()> {
    // Define a mutable variable for if the screen is currently being blocked
    let mut is_screen_blocked = false;

    // Initialise the stream with an initial state
    let mut unified_stream = update_streams_and_state(&conn, &mut is_screen_blocked).await?;

    // Log that the service is monitoring for playback changes in media players
    println!("[PLAYBACK] Media Playback monitor service started");

    loop {
        // If the list of media player streams is empty
        if unified_stream.is_empty() {
            // If no players are found, wait ONLY for the next notification from Task 1
            println!(
                "[PLAYBACK] No active media players found, waiting until nest media player notification..."
            );

            // Wait for the list of media players to update
            if channel_notifier.next().await.is_none() {
                // If the channel has been closed
                return Ok(());
            }

            // Updated the unified set of streams
            unified_stream = update_streams_and_state(&conn, &mut is_screen_blocked).await?;
            continue;
        }

        // Wait for the next signal, either for new media players or playback status change
        futures::select! {
            // If a new media player was added/removed from Task 1
            _ = channel_notifier.next() => {
                // If no players are found, wait ONLY for the next notification from Task 1
                println!("[PLAYBACK] Received discovery signal, refreshing list of media players...");

                // Updated the unified set of streams
                unified_stream = update_streams_and_state(&conn, &mut is_screen_blocked).await?;
            },

            // If the playback status changed for an active player
            result = unified_stream.next() => {
                // If no media players were found
                if result.is_none() {
                    continue;
                }

                // If a media player exists, then update the state to match the changed playback state
                update_blocking_state(&conn, &mut is_screen_blocked).await?;
            },
        }
    }
}

async fn update_streams_and_state(
    conn: &Connection,
    is_screen_blocked: &mut bool,
) -> Result<UnifiedStream> {
    // Get an updated list of streams
    let new_streams = get_media_player_streams(&conn).await?;

    // Update the unified set of streams with the new list
    let unified_stream = select_all(new_streams);

    // Update the blocking state to the current state
    update_blocking_state(&conn, is_screen_blocked).await?;

    // Return the unified set of streams
    Ok(unified_stream)
}
