mod app;
mod global_constants;
mod utils;
mod ui;

use crate::app::application::Application;
use anyhow::Result;
use async_std::task;
use directories::ProjectDirs;
use log::LevelFilter;
use simplelog::{ColorChoice, Config, TermLogger, TerminalMode, WriteLogger};
use std::fs::File;
use std::sync::Arc;
use std::thread;
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tray_icon::menu::{MenuEvent};
use tray_icon::{TrayIconBuilder, Icon};
use crate::ui::system_tray::SystemTrayBuilder;

// Define a custom event type to wake up the loop
enum UserEvent {
    MenuEvent(MenuEvent),
    RefreshIcon
}

// Struct to hold our loaded icons so we don't reload them from disk constantly
struct IconPack {
    active: Icon,
    inactive: Icon,
    blocked: Icon,
}

// Enum to track the current visual state of the icon
#[derive(PartialEq, Clone, Copy, Debug)]
enum AppIconState {
    Active,
    Inactive,
    Blocked,
}

fn main() -> Result<()> {
    // This initializes the GTK backend required by the tray-icon crate
    if let Err(e) = gtk::init() {
        eprintln!("Failed to initialize GTK: {}", e);
        return Err(anyhow::anyhow!("Failed to initialize GTK"));
    }

    // Setup logging to a log file
    log::debug!("[SYSTEM] Setting up log file...");
    let _log_path = setup_logging()?;

    // Create the Application state (Async)
    log::debug!("[SYSTEM] Initializing application state...");
    let app = task::block_on(Application::new())?;

    // Wrap the application state in ARC
    let app = Arc::new(app);
    log::info!("[SYSTEM] Application state initialized successfully");

    // Create a clone of the app for the background process
    let app_worker = app.clone();

    // Spawn a new thread to manage the background processing of media players, and playback state
    log::debug!("[SYSTEM] Spawning background worker thread...");
    thread::spawn(move || {
        task::block_on(async {
            app_worker.run().await;
        });
    });

    // Building the event loop for system tray interactions
    log::debug!("[EVENT LOOP] Building menu event loop...");

    // Create an event loop for the system tray menu
    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();

    // Create a proxy to send events from the tray handler to the menu event loop
    let menu_proxy= event_loop.create_proxy();
    let ui_proxy = menu_proxy.clone();

    // Register the menu event handler
    MenuEvent::set_event_handler(Some(move |event| {
        let _ = menu_proxy.send_event(UserEvent::MenuEvent(event));
    }));

    // Listen for UI updates and forward them to the event loop
    let ui_consumer = app.get_ui_channel().get_consumer();
    thread::spawn(move || {
        task::block_on(async {
            // Wait for messes from the playback monitor
            while let Ok(_) = ui_consumer.recv().await {
                // Wait up the main thread with a RefreshIcon event
                let _ = ui_proxy.send_event(UserEvent::RefreshIcon);
            }
        })
    });

    // Log that the event loop has been registered
    log::info!("[EVENT LOOP] Menu event loop proxy registered successfully");

    // Create a system tray builder
    log::debug!("[TRAY MENU] Creating system tray menu items...");
    let mut tray_builder = SystemTrayBuilder::new();

    // Create the toggle checkbox menu item for blocking screensaver updates
    let toggle_id = tray_builder.create_check_menu_item("Blocker Enabled", true);

    // Add a separator
    tray_builder.create_separator();

    // Create the button to open the logs file
    let logs_id = tray_builder.create_menu_item("Open Logs");

    // Add a separator
    tray_builder.create_separator();

    // Create the button to quit the application
    let quit_id = tray_builder.create_menu_item("Quit");

    // Build the menu
    let tray_menu = tray_builder.build();
    log::info!("[TRAY MENU] System tray menu created successfully");

    // Create a system tray icon
    log::debug!("[TRAY ICON] Building system tray icon...");

    // Load the icon directory
    let icon_dir = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/icons"));

    // Load the icons from the icon directory
    let icons = IconPack {
        active: load_tray_icon(&icon_dir.join("active.png")),
        inactive: load_tray_icon(&icon_dir.join("inactive.png")),
        blocked: load_tray_icon(&icon_dir.join("blocked.png")),
    };

    // Define ths system tray icon + menu
    let tray_icon = TrayIconBuilder::new()
        .with_menu(Box::new(tray_menu))
        .with_tooltip("Media Blocker")
        .with_title("MediaBlocker")
        .with_icon(icons.inactive.clone())
        .build()?;

    // Define the current icon state
    let mut current_icon_state = AppIconState::Inactive;

    // Log that the system tray icon was created successfully
    log::info!("[TRAY ICON] System tray icon created successfully");

    // Get the producer channel for the system tray
    let tray_producer = app.get_tray_channel().get_producer();

    // Start the event loop for the system tray menu
    log::info!("[EVENT LOOP] Starting main event loop...");
    event_loop.run(move |event, _, control_flow| {
        // When loop iteration completes, wait until next event
        *control_flow = ControlFlow::Wait;

        // Receive an event from the menu
        match event {
            // Handle UI refresh requests
            tao::event::Event::UserEvent(UserEvent::RefreshIcon) => {
                // Determine the state of the app icon
                let new_icon_state = determine_app_icon_state(app.clone());

                // If the state has not changes
                if new_icon_state == current_icon_state {
                    // No need to refresh the icon
                    return;
                }

                // Match on the new app icon state
                let new_icon = match new_icon_state {
                    AppIconState::Active => &icons.active,
                    AppIconState::Inactive => &icons.inactive,
                    AppIconState::Blocked => &icons.blocked,
                };

                // Set the tray icon to be the new icon
                let _ = tray_icon.set_icon(Some(new_icon.clone()));

                // Set the current icon to be the new icon
                current_icon_state = new_icon_state;
                log::trace!("[TRAY MENU] New icon: {:?}", new_icon_state);
            }

            // Handle menu item clicks
            tao::event::Event::UserEvent(UserEvent::MenuEvent(menu_event)) => {
                // If the event is to exit the system try
                if menu_event.id == quit_id {
                    log::info!("[SYSTEM TRAY] Quit request received. Exiting application...");
                    *control_flow = ControlFlow::Exit;
                    return;
                }

                // If the event is to toggle the allowing/disallowing of screensaver updates
                if menu_event.id == toggle_id {
                    // Get the opposite state to indicate a toggle
                    let next_state = !app.get_screensaver().are_updates_allowed();

                    log::info!(
                        "[SYSTEM TRAY] Toggle request received. New state: {}",
                        if next_state { "ENABLED" } else { "DISABLED" }
                    );

                    // Update the state of the screensaver to match the system tray state
                    if next_state {
                        app.get_screensaver().allow_updates();
                        log::debug!("[SYSTEM TRAY] Screensaver updates allowed.");
                    } else {
                        app.get_screensaver().disallow_updates();
                        log::debug!("[SYSTEM TRAY] Screensaver updates disallowed.");
                    }

                    // Notify the background worker to adjust state accordingly
                    log::debug!("[SYSTEM TRAY] Sending refresh signal to background worker...");
                    if let Err(e) = task::block_on(tray_producer.send(())) {
                        log::error!("[SYSTEM TRAY] Failed to send signal to worker: {}", e);
                    }
                    return;
                }

                // If the event is to open the log file
                if menu_event.id == logs_id {
                    log::error!("[SYSTEM TRAY] Opening logs button is not a defined action");
                    return;
                }
            }
            _ => {}
        }
    });
}

fn determine_app_icon_state(app: Arc<Application>) -> AppIconState {
    // Get the screensaver from the app
    let screensaver = app.get_screensaver();

    // Get the flags for if the screensaver's blocked/unblocked state can be updated
    let updates_allowed = screensaver.are_updates_allowed();

    // If updates are not allowed
    if !updates_allowed {
        // Update the icon to be in the blocked state
        return AppIconState::Blocked;
    }

    // Get the flag for if the screensave is currently being blocked
    let is_screensaver_blocked = screensaver.is_blocked();

    // If the screensaver is currently being blocked
    if is_screensaver_blocked {
        // Update the icon to be in the active state
        return AppIconState::Active;
    }

    // In the screensaver is not currently being blocked
    // Update the icon to be in the inactive state
    AppIconState::Inactive
}

fn setup_logging() -> Result<std::path::PathBuf> {
    // Match on the state for the parsing of the project directory
    match ProjectDirs::from("com", "MediaBlocker", "MediaBlocker") {
        Some(proj_dirs) => {
            // Get the log directory
            let log_dir = proj_dirs.data_dir();

            // Recursively create the log directory and any parents
            std::fs::create_dir_all(log_dir)?;

            // Get the log file
            let log_file = log_dir.join("media_blocker.log");

            // Create the logger
            simplelog::CombinedLogger::init(vec![
                TermLogger::new(
                    LevelFilter::Warn,
                    Config::default(),
                    TerminalMode::Mixed,
                    ColorChoice::Auto,
                ),
                WriteLogger::new(
                    LevelFilter::Warn,
                    Config::default(),
                    File::create(&log_file)?,
                ),
            ])?;

            // Return the log file
            Ok(log_file)
        }
        None => Err(anyhow::anyhow!("Failed to detect project directory")),
    }
}

fn load_tray_icon(path: &std::path::Path) -> Icon {
    // Load from file
    let (icon_rgba, icon_width, icon_height) = {
        let image = image::open(path)
            .expect("Failed to open icon path")
            .into_rgba8();
        let (width, height) = image.dimensions();
        let rgba = image.into_raw();
        (rgba, width, height)
    };

    // Create icon from RGBA values
    Icon::from_rgba(icon_rgba, icon_width, icon_height).expect("Failed to load icon")
}
