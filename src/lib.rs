//! Get notified when a filesystem is mounted/unmounted!
//!
//! # Getting started
//!
//! The entrypoint of this library is [`MountWatcher`], which enables the detection of
//! mount/unmount events.
//!
//! ```
//! use mount_watch::{MountWatcher, WatchControl};
//!
//! let watch = MountWatcher::new(|event| {
//!     if event.initial {
//!         println!("initial mount points: {:?}", event.mounted);
//!     } else {
//!         println!("new mounts: {:?}", event.mounted);
//!         println!("removed mounts: {:?}", event.unmounted);
//!     }
//!     WatchControl::Continue
//! });
//! // store the watch somewhere (it will stop on drop)
//! ```
//!
//! # Advanced features
//!
//! For more advanced use cases, have a look at [`WatchControl::Coalesce`] and [`callback::coalesce`].

pub mod callback;
pub mod mount;
pub mod watch;

pub use watch::{MountEvent, MountWatcher, WatchControl};

#[cfg(not(target_os = "linux"))]
compile_error!("only Linux is supported");
