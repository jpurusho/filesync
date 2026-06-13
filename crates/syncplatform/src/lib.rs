#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(not(target_os = "macos"))]
compile_error!("MVP targets macOS only");
