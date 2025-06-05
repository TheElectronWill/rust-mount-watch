# Mount watch

Get notified when a filesystem is mounted/unmounted! (Linux only)

Key features:

- Uses `epoll` to watch `/proc/mounts` in an efficient way: no busy polling.
- Emits high-level events with the newly mounted/unmounted filesystems.
- Can be stopped from the event handler, or from the outside.
- Can coalesce multiple events into one, on demand.

## Example

```rs
let watch = MountWatcher::new(|event| {
    if event.initial {
        println!("initial mount points: {:?}", event.mounted);
    } else {
        println!("new mounts: {:?}", event.mounted);
        println!("removed mounts: {:?}", event.unmounted);
    }
    WatchControl::Continue
});
// store the watch somewhere (it will stop on drop)
```
