mod app;
mod global_constants;
mod utils;

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
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{TrayIconBuilder, Icon};

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

    // Build the system tray menu
    log::debug!("[TRAY MENU] Creating system tray menu items...");

    // Toggle for blocking screensaver updates
    let toggle_item = CheckMenuItem::new("Blocker Enabled", true, true, None);

    // Button to open the logs file
    let logs_item = MenuItem::new("Open Logs", true, None);

    // Button for closing the application
    let quit_item = MenuItem::new("Quit", true, None);

    // Create a menu for the system tray that contains the above buttons
    let tray_menu = Menu::new();
    tray_menu.append_items(&[
        &toggle_item,
        &PredefinedMenuItem::separator(),
        &logs_item,
        &PredefinedMenuItem::separator(),
        &quit_item,
    ])?;
    log::info!("[TRAY MENU] System tray menu created successfully");

    // Create a system tray icon
    log::debug!("[TRAY ICON] Building system tray icon...");

    // Load the icon directory
    let icon_dir = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/icons"));

    // Load the icons from the icon directory
    let active_icon = load_tray_icon(&icon_dir.join("active.png"));
    let inactive_icon = load_tray_icon(&icon_dir.join("inactive.png"));
    let blocked_icon = load_tray_icon(&icon_dir.join("blocked.png"));

    // Define the icon pack
    let icons = IconPack {
        active: active_icon,
        inactive: inactive_icon.clone(),
        blocked: blocked_icon,
    };

    // Define ths system tray icon + menu
    let tray_icon = TrayIconBuilder::new()
        .with_menu(Box::new(tray_menu))
        .with_tooltip("Media Blocker")
        .with_title("MediaBlocker")
        .with_icon(inactive_icon.clone())
        .build()?;

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
                // Get the flags for if the screensaver's blocked/unblocked state can be updated
                let updates_allowed = app.get_screensaver().are_updates_allowed();

                // If updates are not allowed
                if !updates_allowed {
                    // Update the icon to be in the blocked state
                    let _ = tray_icon.set_icon(Some(icons.blocked.clone()));
                    return;
                }

                // Get the flag for if the screensave is currently being blocked
                let is_screensaver_blocked = app.get_screensaver().is_blocked();

                // If the screensaver is currently being blocked
                if is_screensaver_blocked {
                    // Update the icon to be in the active state
                    let _ = tray_icon.set_icon(Some(icons.active.clone()));
                } else {
                    // Update the icon to be in the inactive state
                    let _ = tray_icon.set_icon(Some(icons.inactive.clone()));
                }
            }

            // Handle menu item clicks
            tao::event::Event::UserEvent(UserEvent::MenuEvent(menu_event)) => {
                // If the event is to exit the system try
                if menu_event.id == quit_item.id() {
                    log::info!("[SYSTEM TRAY] Quit request received. Exiting application...");
                    *control_flow = ControlFlow::Exit;
                    return;
                }

                // If the event is to toggle the allowing/disallowing of screensaver updates
                if menu_event.id == toggle_item.id() {
                    // Get the opposite state to indicate a toggle
                    let next_state = !app.get_screensaver().are_updates_allowed();

                    log::info!(
                        "[SYSTEM TRAY] Toggle request received. New state: {}",
                        if next_state { "ENABLED" } else { "DISABLED" }
                    );

                    // Update the state of the screensaver to match the system tray state
                    if next_state {
                        app.get_screensaver().allow_updates();
                        let _ = tray_icon.set_icon(Some(icons.active.clone()));
                        log::debug!("[SYSTEM TRAY] Screensaver updates allowed.");
                    } else {
                        app.get_screensaver().disallow_updates();
                        let _ = tray_icon.set_icon(Some(icons.inactive.clone()));
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
                if menu_event.id == logs_item.id() {
                    log::error!("[SYSTEM TRAY] Opening logs button is not a defined action");
                    return;
                }
            }
            _ => {}
        }
    });
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
