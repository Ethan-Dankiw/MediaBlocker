mod global_constants;
mod media_player;
mod screen_blocking;
mod utils;

use crate::global_constants::{ConsumerChannel, MediaPlayerListChangeSignal, ProducerChannel};
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
            }
            Err(e) => {
                eprintln!(
                    "[DISCOVERY] Failed to notify playback monitor of detected changes: {}",
                    e
                )
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
            println!(
                "[PLAYBACK] No active media players found, waiting until nest media player notification..."
            );

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
