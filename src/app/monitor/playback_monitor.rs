use crate::app::application::Application;
use crate::app::media_player::get_media_player_streams;
use crate::global_constants::UnifiedStream;
use futures::stream::select_all;
use futures::StreamExt;
use std::sync::Arc;
use zbus::Connection;

pub struct PlaybackMonitor {}

impl PlaybackMonitor {
    pub async fn start(app: &Arc<Application>) -> anyhow::Result<()> {
        // Extract the D-Bus connection from the app
        let conn = app.get_connection();

        // Extract the screensaver from the app
        let ss = app.get_screensaver();

        // Get the media and system tray consumers from the application
        let mut media_consumer = app.get_media_channel().get_consumer();
        let mut tray_consumer = app.get_tray_channel().get_consumer();

        // Get the UI producer to request the UI be refreshed
        let ui_producer = app.get_ui_channel().get_producer();

        // Initialise the stream with an initial state
        let mut unified_stream = Self::rebuild_streams(conn).await?;

        // Update the state of the application
        ss.update_state(conn).await?;

        // Notify the UI of the initial state
        ui_producer.send(()).await?;

        // Log that the service is monitoring for playback changes in media players
        log::info!("[PLAYBACK] Media Playback monitor service started");

        loop {
            // Wait for the first signal to fire then process it.
            futures::select! {
                // If a signal has been sent from the media producer (MediaMonitor)
                _ = media_consumer.select_next_some() => {
                    // Log that the MediaMonitor detected a change
                    log::trace!("[PLAYBACK] MediaMonitor detected a change");

                    // Rebuild the list of media players since a change has been detected
                    unified_stream = Self::rebuild_streams(conn).await?;
                    ss.update_state(conn).await?;

                    // Request the UI to refresh
                    ui_producer.send(()).await?;
                },

                // If a signal has been sent from the system tray
                _ = tray_consumer.select_next_some() => {
                    // Log that the system tray has asked to refresh state
                    log::trace!("[PLAYBACK] System tray has forced state refresh");

                    // Update the state of the application as system tray has forced update
                    ss.update_state(conn).await?;

                    // Request the UI to refresh
                    ui_producer.send(()).await?;
                }

                // If a signal has been received from an individual media player
                _ = unified_stream.select_next_some() => {
                    // Log that a media player has changed its playback status
                    log::trace!("[PLAYBACK] Media player has changed its playback status");

                    // Update the state of the application as a state change was detected
                    ss.update_state(conn).await?;

                    // Request the UI to refresh
                    ui_producer.send(()).await?;
                }
            }
        }
    }

    async fn rebuild_streams(conn: &Connection) -> anyhow::Result<UnifiedStream> {
        // Get an updated list of streams
        let new_streams = get_media_player_streams(&conn).await?;

        // Update the unified set of streams with the new list
        let unified_stream = select_all(new_streams);

        // Return the unified set of streams
        Ok(unified_stream)
    }
}
