pub fn is_media_player(name: &str) -> bool {
    static FILTER: &str = "org.mpris.MediaPlayer2";
    name.starts_with(FILTER)
}

pub fn is_playback_running(status: &str) -> bool {
    // Return if the status is running
    status.to_lowercase().contains("playing")
}
