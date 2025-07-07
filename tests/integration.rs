use std::time::Duration;

use mount_watcher::{MountEvent, MountWatcher, WatchControl};

/*
NOTE: These tests are for manual testing, because it's quite hard to automate it (it requires to mount/unmount filesystems as root).

To run, remove the ignore attribute and run:
RUST_LOG=debug cargo test --package mount-watcher --test integration -- watch_coalesce_print --exact --show-output --nocapture

And, in parallel (in another terminal), mount and unmount filesystems to see if the callback is called at the right time and with the right arguments.
*/

#[ignore]
#[test]
fn watch_print() {
    env_logger::init();

    let watch = MountWatcher::new(|event| {
        print_event(event);
        println!("---------------");

        WatchControl::Continue
    })
    .unwrap();
    std::thread::sleep(Duration::from_secs(30));
    log::debug!("stopping");
    watch.stop().expect("stop should work");
    log::debug!("stopped");
    watch
        .join()
        .expect("no error should be reported by the polling loop");
}

#[ignore]
#[test]
fn stop_by_dropping() {
    env_logger::init();

    let watch = MountWatcher::new(|event| {
        print_event(event);
        println!("---------------");

        WatchControl::Continue
    })
    .unwrap();
    std::thread::sleep(Duration::from_secs(5));
    drop(watch);
}

#[ignore]
#[test]
fn watch_coalesce_print() {
    env_logger::init();

    let watch = MountWatcher::new(|event| {
        if event.initial {
            println!("got initial event");
            return WatchControl::Continue;
        }

        if !event.coalesced {
            println!("not coalesced => coalesce for 5s!");
            return WatchControl::Coalesce {
                delay: Duration::from_secs(5),
            };
        }

        print_event(event);
        WatchControl::Stop
    })
    .unwrap();
    watch.stop().unwrap();
    watch.join().unwrap();
}

fn print_event(event: MountEvent) {
    println!("coalesced: {}, initial: {}", event.coalesced, event.initial);
    println!(
        "mounted:\n\t{}",
        event
            .mounted
            .iter()
            .map(|m| format!("{m:?}"))
            .collect::<Vec<String>>()
            .join("\n\t")
            .to_string()
    );
    println!(
        "unmounted:\n\t{}",
        event
            .unmounted
            .iter()
            .map(|m| format!("{m:?}"))
            .collect::<Vec<String>>()
            .join("\n\t")
            .to_string()
    );
}
