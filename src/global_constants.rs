use async_std::stream::Stream;
use futures::stream::SelectAll;

// Paths to DBus object
pub const DBUS_DESTINATION: &str = "org.freedesktop.DBus";
pub const DBUS_PATH: &str = "/org/freedesktop/DBus";
pub const DBUS_INTERFACE: &str = "org.freedesktop.DBus";

// Paths to the MediaPlayer object
pub const MEDIA_PLAYER_PATH: &str = "/org/mpris/MediaPlayer2";
pub const MEDIA_PLAYER_INTERFACE: &str = "org.freedesktop.DBus.Properties";

// Paths to the Idle Inhibition Service (ScreenSaver)
pub const SCREENSAVER_DESTINATION: &str = "org.freedesktop.ScreenSaver";
pub const SCREENSAVER_PATH: &str = "/org/freedesktop/ScreenSaver";
pub const SCREENSAVER_INTERFACE: &str = "org.freedesktop.ScreenSaver";

// Type alias for the stream of D-Bus messages
pub type DbusSignalStream = std::pin::Pin<Box<dyn Stream<Item = zbus::Message> + Send>>;

// Type alias for a set of all streams
pub type UnifiedStream = SelectAll<DbusSignalStream>;

// Struct to indicate to the consumer which action to perform (addition/removal of a player)
pub type MediaPlayerListChangeSignal = ();

// Type alias for the producer channel
pub type ProducerChannel = async_std::channel::Sender<MediaPlayerListChangeSignal>;

// Type alias for the consumer channel
pub type ConsumerChannel = async_std::channel::Receiver<MediaPlayerListChangeSignal>;
