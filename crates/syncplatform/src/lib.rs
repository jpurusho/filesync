#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(target_os = "macos")]
pub use macos::{app_db_path, identity_path};

#[cfg(not(target_os = "macos"))]
compile_error!("MVP targets macOS only");
