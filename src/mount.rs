//! Parse /proc/mounts.

use std::{
    fs::File,
    io::{Read, Seek},
};

use thiserror::Error;

pub const PROC_MOUNTS_PATH: &str = "/proc/mounts";

/// A mounted filesystem.
///
/// See `man fstab` for a detailed description of the fields.
#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub struct LinuxMount {
    pub spec: String,
    pub mount_point: String,
    pub fs_type: String,
    pub mount_options: Vec<String>,
    pub dump_fs_freq: u32,
    pub fsck_fs_passno: u32,
}

/// Error while parsing `/proc/mounts`.
#[derive(Debug, Error)]
#[error("invalid mount line: {input}")]
pub struct ParseError {
    pub(crate) input: String,
}

/// Error while reading/parsing `/proc/mounts`.
#[derive(Debug, Error)]
pub enum ReadError {
    #[error("failed to parse {PROC_MOUNTS_PATH}")]
    Parse(#[from] ParseError),
    #[error("failed to read {PROC_MOUNTS_PATH}")]
    Io(#[from] std::io::Error),
}

impl LinuxMount {
    /// Attempts to parse a line of `/proc/mounts`.
    /// Returns `None` if it fails.
    pub(crate) fn parse(line: &str) -> Option<Self> {
        let mut fields = line.split_ascii_whitespace().into_iter();
        let spec = fields.next()?.to_string();
        let mount_point = fields.next()?.to_string();
        let fs_type = fields.next()?.to_string();
        let mount_options = fields.next()?.split(',').map(ToOwned::to_owned).collect();
        let dump_fs_freq = fields.next()?.parse().ok()?;
        let fsck_fs_passno = fields.next()?.parse().ok()?;
        Some(Self {
            spec,
            mount_point,
            fs_type,
            mount_options,
            dump_fs_freq,
            fsck_fs_passno,
        })
    }
}

/// Reads `/proc/mounts` from the beginning and parses its content.
pub(crate) fn read_proc_mounts(file: &mut File) -> Result<Vec<LinuxMount>, ReadError> {
    let mut content = String::with_capacity(4096);
    file.rewind()?;
    file.read_to_string(&mut content).map_err(ReadError::from)?;
    let mut mounts = Vec::with_capacity(64);
    parse_proc_mounts(&content, &mut mounts).map_err(ReadError::from)?;
    Ok(mounts)
}

/// Parses the content of `/proc/mounts`.
pub(crate) fn parse_proc_mounts(
    content: &str,
    buf: &mut Vec<LinuxMount>,
) -> Result<(), ParseError> {
    for line in content.lines() {
        let line = line.trim_start_matches(|c: char| c.is_ascii_whitespace());
        if !line.is_empty() && !line.starts_with('#') {
            let m = LinuxMount::parse(line).ok_or_else(|| ParseError {
                input: line.to_owned(),
            })?;
            buf.push(m);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::{parse_proc_mounts, LinuxMount};

    fn vec_str(values: &[&str]) -> Vec<String> {
        values.into_iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parsing() {
        let content = "
sysfs /sys sysfs rw,nosuid,nodev,noexec,relatime 0 0
tmpfs /run tmpfs rw,nosuid,nodev,noexec,relatime,size=1599352k,mode=755,inode64 1 2
cgroup2 /sys/fs/cgroup cgroup2 rw,nosuid,nodev,noexec,relatime,nsdelegate,memory_recursiveprot 0 0
/dev/nvme0n1p1 /boot/efi vfat rw,relatime,errors=remount-ro 0 0";
        let mut mounts = Vec::new();
        parse_proc_mounts(&content, &mut mounts).unwrap();

        let expected = vec![
            LinuxMount {
                spec: String::from("sysfs"),
                mount_point: String::from("/sys"),
                fs_type: String::from("sysfs"),
                mount_options: vec_str(&["rw", "nosuid", "nodev", "noexec", "relatime"]),
                dump_fs_freq: 0,
                fsck_fs_passno: 0,
            },
            LinuxMount {
                spec: String::from("tmpfs"),
                mount_point: String::from("/run"),
                fs_type: String::from("tmpfs"),
                mount_options: vec_str(&[
                    "rw",
                    "nosuid",
                    "nodev",
                    "noexec",
                    "relatime",
                    "size=1599352k",
                    "mode=755",
                    "inode64",
                ]),
                dump_fs_freq: 1,
                fsck_fs_passno: 2,
            },
            LinuxMount {
                spec: String::from("cgroup2"),
                mount_point: String::from("/sys/fs/cgroup"),
                fs_type: String::from("cgroup2"),
                mount_options: vec_str(&[
                    "rw",
                    "nosuid",
                    "nodev",
                    "noexec",
                    "relatime",
                    "nsdelegate",
                    "memory_recursiveprot",
                ]),
                dump_fs_freq: 0,
                fsck_fs_passno: 0,
            },
            LinuxMount {
                spec: String::from("/dev/nvme0n1p1"),
                mount_point: String::from("/boot/efi"),
                fs_type: String::from("vfat"),
                mount_options: vec_str(&["rw", "relatime", "errors=remount-ro"]),
                dump_fs_freq: 0,
                fsck_fs_passno: 0,
            },
        ];
        assert_eq!(expected, mounts);
    }

    #[test]
    fn parsing_error() {
        let mut mounts = Vec::new();
        parse_proc_mounts("badbad", &mut mounts).unwrap_err();
        parse_proc_mounts("croup2 /sys/fs/cgroup", &mut mounts).unwrap_err();
    }

    #[test]
    fn parsing_comments() {
        let mut mounts = Vec::new();
        parse_proc_mounts("\n# badbad\n", &mut mounts).unwrap();
    }
}
