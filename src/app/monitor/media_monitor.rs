use crate::app::application::Application;
use crate::utils::is_media_player;
use futures::StreamExt;
use std::sync::Arc;
use zbus::fdo::DBusProxy;

pub struct MediaMonitor {}

impl MediaMonitor {
    pub async fn start(app: &Arc<Application>) -> anyhow::Result<()> {
        // Extract the D-Bus connection from the app
        let conn = app.get_connection();

        // Create a proxy for the D-Bus interface
        let dbus: DBusProxy = DBusProxy::new(conn).await?;

        // Receive all the signals matching the added rule
        let mut signal_stream = dbus.receive_name_owner_changed().await?;

        // Log that the service is monitoring for media player signals
        log::info!("[DISCOVERY] Media Player discovery service started");

        // Process the signals
        while let Some(signal) = signal_stream.next().await {
            // Deserialize the message arguments into the three known NameOwnerChanged strings.
            let args = match signal.args() {
                Ok(a) => a,
                Err(e) => {
                    log::error!("[DISCOVERY] Failed to deserialize signal arguments: {}", e);
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
            log::debug!("[DISCOVERY] Detected change in list of media players");

            // Extract the old and new owners of the service
            let old_owner = args.old_owner;
            let new_owner = args.new_owner;

            // Log the action that is taken for the changed list
            if old_owner.is_none() && new_owner.is_some() {
                log::trace!("[DISCOVERY] {} has been added", service_name);
            } else if old_owner.is_some() && new_owner.is_none() {
                log::trace!("[DISCOVERY] {} has been removed", service_name);
            }

            // Extract the producer for notifying the playback monitor of changes to list of media players
            let producer = app.get_media_channel().get_producer();

            // Send a signal to Task 2 to rebuild its list of media players
            match producer.send(()).await {
                Ok(_) => {
                    log::debug!(
                        "[DISCOVERY] Playback monitor has been notified of detected changes"
                    )
                }
                Err(e) => {
                    log::error!(
                        "[DISCOVERY] Failed to notify playback monitor of detected changes: {}",
                        e
                    );
                    break;
                }
            }
        }

        Ok(())
    }
}
