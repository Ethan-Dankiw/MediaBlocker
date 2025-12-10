use crate::global_constants::{
    DBUS_DESTINATION, DBUS_INTERFACE, DBUS_PATH, MEDIA_PLAYER_INTERFACE, MEDIA_PLAYER_PATH,
};
use zbus::{Connection, Proxy};
use zvariant::OwnedValue;

pub fn is_media_player(name: &str) -> bool {
    static FILTER: &str = "org.mpris.MediaPlayer2";
    name.starts_with(FILTER)
}

pub fn is_playback_running(status: &str) -> bool {
    // Return if the status is running
    status.to_lowercase().contains("playing")
}

pub async fn get_media_player_names(conn: &Connection) -> anyhow::Result<Vec<String>> {
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

pub async fn get_playback_status(
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
