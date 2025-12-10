use crate::app::monitor::channel::AppChannel;
use crate::app::monitor::media_monitor::MediaMonitor;
use crate::app::monitor::playback_monitor::PlaybackMonitor;
use crate::app::screensaver::ScreensaverState;
use std::sync::Arc;
use zbus::Connection;

// Type alias for a signal that indicates that the list of media players has changes
pub type MediaPlayerListChangeSignal = ();

// Type alias for a signal that indicates that the system tray has updated a screensaver state
pub type SystemTrayRefreshScreensaverSignal = ();

// Type alias for a signal that is sent to the UI to request an icon refresh
pub type UiRefreshSignal = ();

pub struct Application {
    /// Connection to the D-Bus session
    connection: Connection,

    /// The blocked/unblocked state of the screensaver
    screensaver: Arc<ScreensaverState>,

    /// The channel for the system tray
    tray_channel: AppChannel<SystemTrayRefreshScreensaverSignal>,

    /// The channel for the media player
    media_channel: AppChannel<MediaPlayerListChangeSignal>,

    /// The channel for the UI refresh notification
    ui_channel: AppChannel<UiRefreshSignal>,
}

impl Application {
    pub async fn new() -> anyhow::Result<Self> {
        // Establish a connection to the D-Bus session
        let conn = Connection::session().await?;

        // Construct the ApplicationState instance
        Ok(Self {
            connection: conn,
            screensaver: Arc::new(ScreensaverState::new()),
            tray_channel: AppChannel::new(),
            media_channel: AppChannel::new(),
            ui_channel: AppChannel::new(),
        })
    }

    pub fn get_connection(&self) -> &Connection {
        &self.connection
    }

    pub fn get_screensaver(&self) -> &Arc<ScreensaverState> {
        &self.screensaver
    }

    pub fn get_tray_channel(&self) -> &AppChannel<SystemTrayRefreshScreensaverSignal> {
        &self.tray_channel
    }

    pub fn get_media_channel(&self) -> &AppChannel<MediaPlayerListChangeSignal> {
        &self.media_channel
    }

    pub fn get_ui_channel(&self) -> &AppChannel<UiRefreshSignal> {
        &self.ui_channel
    }

    pub async fn run(self: Arc<Self>) {
        log::info!("[SYSTEM] MediaBlocker starting...");

        // Monitor the additional/removal of media players from the D-Bus
        log::debug!(
            "[SYSTEM] Spawning Media Monitor to track the addition/removal of media players..."
        );
        let media_app = self.clone();
        async_std::task::spawn(async move {
            if let Err(e) = MediaMonitor::start(&media_app).await {
                log::error!("[DISCOVERY] Media Monitor has crashed: {}", e)
            }
        });

        // Monitor the playback status of active media players
        log::debug!(
            "[SYSTEM] Spawning Playback Monitor to track playback status of active media players..."
        );
        let playback_app = self.clone();
        async_std::task::spawn(async move {
            if let Err(e) = PlaybackMonitor::start(&playback_app).await {
                log::error!("[PLAYBACK] Playback Monitor has crashed: {}", e)
            }
        });
    }
}
