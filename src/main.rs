//! The Kensington Expert Trackball control panel — an eframe (egui) desktop app for Linux,
//! built for Wayland (Hyprland) with an X11 fallback.
//!
//! The same program does two other jobs, picked by flags:
//! - `--helper`: the daemon that makes your buttons and ring settings live (the panel
//!   starts it for you).
//! - `--screenshot <file.png>`: draw about 120 frames, save the window's picture and exit
//!   (what the automated build checks use).

mod app;
mod device;
mod helper;
mod keys;
mod settings;
mod system;

fn main() -> eframe::Result {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("--helper") => match helper::run() {
            Ok(()) => std::process::exit(0),
            Err(e) => {
                eprintln!("trackball helper: {e}");
                std::process::exit(1);
            }
        },
        Some("--version") | Some("-V") => {
            println!("kensington-trackball panel {}", env!("CARGO_PKG_VERSION"));
            std::process::exit(0);
        }
        Some("--screenshot") => {
            // The panel runs normally, just with a picture to save at the end.
            match args.next() {
                Some(path) => run_panel(Some(std::path::PathBuf::from(path))),
                None => {
                    eprintln!("--screenshot needs a file name, e.g. --screenshot shot.png");
                    std::process::exit(2);
                }
            }
        }
        Some(other) if other.starts_with('-') => {
            println!("Usage: kensington-trackball [--helper | --screenshot <file.png> | --version]");
            std::process::exit(2);
        }
        _ => run_panel(None),
    }
}

fn run_panel(screenshot: Option<std::path::PathBuf>) -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Kensington Expert Trackball")
            .with_inner_size([940.0, 680.0]),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    let screenshot_mode = screenshot.is_some();
    let result = eframe::run_native(
        "Kensington Expert Trackball",
        options,
        Box::new(move |_cc| Ok(Box::new(app::Panel::new(screenshot)))),
    );
    // In screenshot mode the program exits by itself once the picture is on disk, so a clean
    // end here means the window closed (or the run was wound down) before a picture could be
    // saved; say so loudly instead of ending with the picture unsaved.
    if screenshot_mode && result.is_ok() {
        eprintln!("screenshot: the window closed before a picture could be saved");
        std::process::exit(3);
    }
    result
}
