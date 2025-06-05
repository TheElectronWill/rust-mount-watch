//! Callback helpers.

use std::time::Duration;

use crate::{MountEvent, WatchControl};

/// How to handle the initial event, which contains the list of mount points that
/// have been detected when the watcher has started.
pub enum CoalesceInitial {
    /// Coalesce the initial event like any other event.
    Coalesce,
    /// Do not coalesce the initial event, only the subsequent events.
    PassImmediately,
}

/// Returns a closure that always coalesce the events with the given delay.
///
/// By passing it to [`MountWatcher::new`](crate::MountWatcher::new), you will only
/// get events at the specified time interval.
///
/// # Initial event
///
/// The first, initial event is handled according to the value of `initial_event`.
/// It can be useful to use `PassImmediately` to get a first list of the mount points
/// as soon as possible.
///
/// # Example
///
/// ```no_run
/// use std::time::Duration;
/// use mount_watch::MountWatcher;
/// use mount_watch::callback::{coalesce, CoalesceInitial};
///
/// let watch = MountWatcher::new(
///     coalesce(
///         Duration::from_secs(5),
///         CoalesceInitial::PassImmediately,
///         |event| {
///             todo!("handle event")
///         }
///     )
/// );
/// ```
pub fn coalesce<F: FnMut(MountEvent) -> WatchControl + Send + 'static>(
    delay: Duration,
    initial_event: CoalesceInitial,
    mut f: F,
) -> impl FnMut(MountEvent) -> WatchControl + Send + 'static {
    move |event| {
        let coalesce = match initial_event {
            CoalesceInitial::Coalesce => !event.coalesced,
            CoalesceInitial::PassImmediately => !(event.coalesced || event.initial),
        };
        if coalesce {
            WatchControl::Coalesce { delay }
        } else {
            f(event)
        }
    }
}
