//! Wait for the cgroupfs to be mounted.

pub mod callback;
pub mod mount;
pub mod watch;

pub use watch::{MountEvent, MountWatch, WatchControl};

#[cfg(not(target_os = "linux"))]
compile_error!("only Linux is supported");
