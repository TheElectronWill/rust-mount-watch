use std::time::Duration;

use mount_watch::{MountEvent, MountWatch, WatchControl};

/*
NOTE: These tests are for manual testing, because it's quite hard to automate it (it requires to mount/unmount filesystems as root).

To run, execute:
RUST_LOG=debug cargo test --package mount-watch --test integration -- watch_coalesce_print --exact --show-output --nocapture

And, in parallel (in another terminal), mount and unmount filesystems to see if the callback is called at the right time and with the right arguments.
*/

#[test]
fn watch_print() {
    env_logger::init();

    let watch = MountWatch::new(|event| {
        print_event(event);
        println!("---------------");

        WatchControl::Continue
    })
    .unwrap();
    std::thread::sleep(Duration::from_secs(60));
}

#[test]
fn watch_coalesce_print() {
    env_logger::init();

    let watch = MountWatch::new(|event| {
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
