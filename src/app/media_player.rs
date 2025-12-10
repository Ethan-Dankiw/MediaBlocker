use crate::global_constants::{DbusSignalStream, DBUS_DESTINATION, DBUS_INTERFACE, DBUS_PATH, MEDIA_PLAYER_INTERFACE, MEDIA_PLAYER_PATH};
use crate::utils::{is_media_player, is_playback_running};
use zbus::{Connection, Proxy};
use zvariant::OwnedValue;

pub async fn get_media_player_streams(conn: &Connection) -> anyhow::Result<Vec<DbusSignalStream>> {
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

pub async fn any_playing_media(conn: &Connection) -> anyhow::Result<bool> {
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



async fn get_media_player_names(conn: &Connection) -> anyhow::Result<Vec<String>> {
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

async fn get_playback_status(
    conn: &Connection,
    player: &str,
) -> anyhow::Result<Option<String>> {
    // Open a proxy layer to the D-Bus to interface with its methods or properties
    let properties = Proxy::new(&conn, player, MEDIA_PLAYER_PATH, MEDIA_PLAYER_INTERFACE).await?;

    // Get the playback status from the player
    let body = ("org.mpris.MediaPlayer2.Player", "PlaybackStatus");
    let status: anyhow::Result<OwnedValue, _> = properties.call("Get", &body).await;

    // Check for the existence of the property
    match status {
        Ok(value) => Ok(Some(value.to_string())),
        Err(_) => Ok(None),
    }
}
