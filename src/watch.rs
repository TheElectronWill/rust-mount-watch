//! Main module.

use std::{
    collections::HashSet,
    fs::File,
    io::ErrorKind,
    os::fd::AsRawFd,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::JoinHandle,
    time::Duration,
};

use mio::{unix::SourceFd, Events, Interest, Poll, Token};
use thiserror::Error;
use timerfd::TimerFd;

use crate::mount::ReadError;

use super::mount::{read_proc_mounts, LinuxMount};

/// `MountWatcher` allows to react to changes in the mounted filesystems.
///
/// # Stopping
///
/// When the `MountWatcher` is dropped, the background thread that drives the watch is stopped, and the callback will never be called again.
/// You can also call [`stop`](Self::stop).
///
/// Furthermore, you can stop the watch from the event handler itself, by returning [`WatchControl::Stop`].
///
/// # Example (stop in handler)
///
/// ```no_run
/// use mount_watch::{MountWatcher, WatchControl};
///
/// let watch = MountWatcher::new(|event| {
///     let added_mounts = event.mounted;
///     let removed_mounts = event.unmounted;
///     let stop_condition = todo!();
///     if stop_condition {
///         // I have found what I wanted, stop here.
///         WatchControl::Stop
///     } else {
///         // Continue to watch, I still want events.
///         WatchControl::Continue
///     }
/// }).unwrap();
/// // Wait for the watch to be stopped by the handler
/// watch.join().unwrap();
/// ```
pub struct MountWatcher {
    thread_handle: Option<JoinHandle<()>>,
    stop_flag: Arc<AtomicBool>,
}

/// Error in `MountWatcher` setup.
#[derive(Debug, Error)]
#[error("MountWatcher setup error")]
pub struct SetupError(#[source] ErrorImpl);

/// Private error type: I don't want to expose it for the moment.
#[derive(Debug, Error)]
enum ErrorImpl {
    #[error("read error")]
    MountRead(#[from] ReadError),
    #[error("failed to initialize epoll")]
    PollInit(#[source] std::io::Error),
    #[error("poll.poll() returned an error")]
    PollPoll(#[source] std::io::Error),
    #[error("failed to register a timer to epoll")]
    PollTimer(#[source] std::io::Error),
    #[error("could not set up a timer with delay {0:?} for event coalescing")]
    Timerfd(Duration, #[source] std::io::Error),
}

impl MountWatcher {
    /// Watches the list of mounted filesystems and executes the `callback` when it changes.
    pub fn new(
        callback: impl FnMut(MountEvent) -> WatchControl + Send + 'static,
    ) -> Result<Self, SetupError> {
        watch_mounts(callback).map_err(SetupError)
    }

    /// Stops the waiting thread and wait for it to terminate.
    ///
    /// # Errors
    /// If the thread has panicked, an error is returned with the panic payload.
    pub fn stop(mut self) -> std::thread::Result<()> {
        self.stop_flag.store(true, Ordering::Relaxed);
        self.thread_handle.take().unwrap().join()
    }

    pub fn join(mut self) -> std::thread::Result<()> {
        self.thread_handle.take().unwrap().join()
    }
}

impl Drop for MountWatcher {
    fn drop(&mut self) {
        if self.thread_handle.is_some() {
            self.stop_flag.store(true, Ordering::Relaxed);
        }
    }
}

/// Event generated when the mounted filesystems change.
pub struct MountEvent {
    /// The new filesystems that have been mounted.
    pub mounted: Vec<LinuxMount>,

    /// The old filesystems that have been unmounted.
    pub unmounted: Vec<LinuxMount>,

    /// Indicates whether this is a coalesced event.
    ///
    /// See [`WatchControl::Coalesce`].
    pub coalesced: bool,

    /// Indicates whether this is the first event, which contains
    /// the list of all the mounts.
    pub initial: bool,
}

/// Value returned by the event handler to control the [`MountWatcher`].
pub enum WatchControl {
    /// Continue watching.
    Continue,
    /// Stop watching.
    Stop,
    /// After the given delay, call the callback again.
    ///
    /// In the event, the current mounts/unmounts will be included, in addition to the
    /// new mounts/unmounts that will occur during the delay.
    Coalesce { delay: Duration },
}

const MOUNT_TOKEN: Token = Token(0);
const TIMER_TOKEN: Token = Token(1);
const POLL_TIMEOUT: Duration = Duration::from_secs(5);
const PROC_MOUNTS_PATH: &str = "/proc/mounts";

struct State<F: FnMut(MountEvent) -> WatchControl> {
    known_mounts: HashSet<LinuxMount>,
    callback: F,
    coalesce_timer: Option<TimerFd>,
    coalescing: bool,
}

impl<F: FnMut(MountEvent) -> WatchControl> State<F> {
    fn new(callback: F) -> Self {
        Self {
            known_mounts: HashSet::with_capacity(8),
            callback,
            coalesce_timer: None,
            coalescing: false,
        }
    }

    fn check_diff(
        &mut self,
        file: &mut File,
        coalesced: bool,
        initial: bool,
    ) -> Result<WatchControl, ReadError> {
        debug_assert!(
            !(coalesced && !self.coalescing),
            "inconsistent state: coalescing flag should be set before setting the trigger up"
        );
        if self.coalescing {
            if coalesced {
                // The timer has been triggered, clear the flag.
                self.coalescing = false;
            } else {
                // We are coalescing the events, wait for the timer.
                return Ok(WatchControl::Continue);
            }
        }

        let mounts = read_proc_mounts(file)?;
        let mounts = HashSet::from_iter(mounts);
        let unmounted: Vec<&LinuxMount> = self.known_mounts.difference(&mounts).collect();
        let mounted: Vec<&LinuxMount> = mounts.difference(&self.known_mounts).collect();
        log::trace!("known_mounts: {:?}", self.known_mounts);
        log::trace!("curr. mounts: {:?}", mounts);

        if mounted.is_empty() && unmounted.is_empty() {
            // Weird: we got a notification but nothing has changed?
            // Perhaps something was undone between the moment we got the notification and
            // the moment we read the /proc/mounts virtual file?
            log::warn!("nothing changed");
            return Ok(WatchControl::Continue);
        }

        // call the callback with the changes
        let event = MountEvent {
            mounted: mounted.into_iter().cloned().collect(),
            unmounted: unmounted.into_iter().cloned().collect(),
            coalesced,
            initial,
        };
        let res = (self.callback)(event);
        if !matches!(res, WatchControl::Coalesce { .. }) {
            // When coalescing, don't save the new mounts, we'll compute
            // the difference again and send the future result instead.
            // On the contrary, when NOT coalescing, save the new mounts.
            self.known_mounts = mounts;
        }
        // propagate the choice of the callback
        Ok(res)
    }

    fn start_coalescing(&mut self, delay: Duration, poll: &Poll) -> Result<(), ErrorImpl> {
        log::trace!("start coalescing for {delay:?}");
        let mut register = false;
        if self.coalesce_timer.is_none() {
            // create the timer, don't register it yet because it is not configured
            self.coalesce_timer = Some(TimerFd::new().map_err(|e| ErrorImpl::Timerfd(delay, e))?);
            register = true;
            log::trace!("timerfd created");
        }

        // configure the timer
        let timer = self.coalesce_timer.as_mut().unwrap();
        timer.set_state(
            timerfd::TimerState::Oneshot(delay),
            timerfd::SetTimeFlags::Default,
        );

        // register the timer to the epoll instance
        if register {
            let fd = timer.as_raw_fd();
            let mut source = SourceFd(&fd);
            poll.registry()
                .register(&mut source, TIMER_TOKEN, Interest::READABLE)
                .map_err(ErrorImpl::PollTimer)?;
            log::trace!("timerfd registered");
        }
        // set the coalescing flag
        self.coalescing = true;
        Ok(())
    }
}

/// Starts a background thread that uses [`mio::poll`] (backed by `epoll`) to detect changes to the mounted filesystem.
fn watch_mounts<F: FnMut(MountEvent) -> WatchControl + Send + 'static>(
    callback: F,
) -> Result<MountWatcher, ErrorImpl> {
    // Open the file that contains info about the mounted filesystems.
    let mut file =
        File::open(PROC_MOUNTS_PATH).map_err(|e| ErrorImpl::MountRead(ReadError::Io(e)))?;
    let fd = file.as_raw_fd();
    let mut fd = SourceFd(&fd);

    // Prepare epoll.
    // According to `man proc_mounts`, a filesystem mount or unmount causes
    // `poll` and `epoll_wait` to mark the file as having a PRIORITY event.
    let mut poll = Poll::new().map_err(|e| ErrorImpl::PollInit(e))?;
    poll.registry()
        .register(&mut fd, MOUNT_TOKEN, Interest::PRIORITY)
        .map_err(|e| ErrorImpl::PollInit(e))?;

    // Keep a boolean to stop the thread from the outside.
    let stop_flag = Arc::new(AtomicBool::new(false));
    let stop_flag_thread = stop_flag.clone();

    // Declare the polling loop separately to handle errors in a nicer way.
    let poll_loop = move || -> Result<(), ErrorImpl> {
        let mut events = Events::with_capacity(8); // we don't expect many events
        let mut state = State::new(callback);

        // While we were setting up epoll, some filesystems may have been mounted.
        // Check that here to avoid any miss.
        match state.check_diff(&mut file, false, true)? {
            WatchControl::Continue => (),
            WatchControl::Stop => return Ok(()),
            WatchControl::Coalesce { delay } => {
                state.start_coalescing(delay, &poll)?;
            }
        }

        loop {
            let poll_res = poll.poll(&mut events, Some(POLL_TIMEOUT));
            if let Err(e) = poll_res {
                if e.kind() == ErrorKind::Interrupted {
                    continue; // retry
                } else {
                    return Err(ErrorImpl::PollPoll(e)); // propagate error
                }
            }

            // Call next() because we are not interested in each individual event.
            // If the timeout elapses, the event list is empty.
            if let Some(event) = events.iter().next() {
                log::debug!("event on /proc/mounts: {event:?}");

                // parse mount file and react to changes
                let coalesced = dbg!(event.token() == TIMER_TOKEN);
                match state.check_diff(&mut file, coalesced, false)? {
                    WatchControl::Continue => (),
                    WatchControl::Stop => break,
                    WatchControl::Coalesce { delay } => {
                        state.start_coalescing(delay, &poll)?;
                    }
                }
            }
            if stop_flag_thread.load(Ordering::Relaxed) {
                break;
            }
        }
        Ok(())
    };

    // Spawn a thread.
    let thread_handle = std::thread::spawn(move || {
        if let Err(e) = poll_loop() {
            log::error!("error in poll loop: {e:?}");
        }
    });

    // Return a structure that will stop the polling when dropped.
    Ok(MountWatcher {
        thread_handle: Some(thread_handle),
        stop_flag,
    })
}
