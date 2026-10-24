#[cfg(feature = "ui")]
mod application;
#[cfg(feature = "ui")]
mod config;
#[cfg(feature = "ui")]
mod crop_canvas;
pub mod crop_state;
pub mod image_processor;
#[cfg(feature = "ui")]
mod window;

#[cfg(feature = "ui")]
use application::CropApplication;
#[cfg(feature = "ui")]
use gio::prelude::*;

#[cfg(feature = "ui")]
fn main() -> glib::ExitCode {
    libadwaita::init().expect("Failed to initialize libadwaita");
    let app = CropApplication::new();
    app.run()
}

#[cfg(not(feature = "ui"))]
fn main() {
    println!("Crop Picture built without UI feature. Build with --features ui to run GUI.");
}
