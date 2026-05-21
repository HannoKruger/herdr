//! Platform-specific process and filesystem operations.
//!
//! Centralizes OS-dependent behavior behind a clean boundary so core
//! modules don't scatter `#[cfg]` branches through product logic.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForegroundProcess {
    pub pid: u32,
    pub name: String,
    pub argv0: Option<String>,
    pub cmdline: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForegroundJob {
    pub process_group_id: u32,
    pub processes: Vec<ForegroundProcess>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signal {
    Hangup,
    Terminate,
    Kill,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardCommand {
    pub program: &'static str,
    pub args: &'static [&'static str],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardImage {
    pub bytes: Vec<u8>,
    pub extension: &'static str,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum LimitedRead {
    Empty,
    Complete(Vec<u8>),
    Oversized,
}

pub(crate) fn read_limited_reader(
    mut reader: impl std::io::Read,
    max_bytes: usize,
) -> std::io::Result<LimitedRead> {
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 8192];

    while bytes.len() < max_bytes {
        let remaining = max_bytes - bytes.len();
        let read_len = remaining.min(buffer.len());
        let bytes_read = match reader.read(&mut buffer[..read_len]) {
            Ok(bytes_read) => bytes_read,
            Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(err) => return Err(err),
        };
        if bytes_read == 0 {
            return if bytes.is_empty() {
                Ok(LimitedRead::Empty)
            } else {
                Ok(LimitedRead::Complete(bytes))
            };
        }
        bytes.extend_from_slice(&buffer[..bytes_read]);
    }

    let mut sentinel = [0_u8; 1];
    loop {
        return match reader.read(&mut sentinel) {
            Ok(0) if bytes.is_empty() => Ok(LimitedRead::Empty),
            Ok(0) => Ok(LimitedRead::Complete(bytes)),
            Ok(_) => Ok(LimitedRead::Oversized),
            Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(err) => Err(err),
        };
    }
}

/// Extract `key`'s value from a NUL-separated `KEY=VALUE` environment block —
/// the format of Linux `/proc/<pid>/environ` and the env section of the macOS
/// `KERN_PROCARGS2` buffer. Reads a process's environment, which is fixed at
/// exec time, so it is a stable per-process identifier (unlike open fds).
pub(crate) fn parse_environ_var(environ: &[u8], key: &str) -> Option<String> {
    let mut prefix = Vec::with_capacity(key.len() + 1);
    prefix.extend_from_slice(key.as_bytes());
    prefix.push(b'=');
    environ
        .split(|&byte| byte == 0)
        .find(|entry| entry.starts_with(&prefix))
        .map(|entry| String::from_utf8_lossy(&entry[prefix.len()..]).into_owned())
}

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::*;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::*;

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
mod fallback;
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub use fallback::*;

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a NUL-separated environ block, like `/proc/<pid>/environ`.
    fn environ_block(entries: &[&str]) -> Vec<u8> {
        let mut buf = Vec::new();
        for entry in entries {
            buf.extend_from_slice(entry.as_bytes());
            buf.push(0);
        }
        buf
    }

    #[test]
    fn parse_environ_var_extracts_value() {
        let env = environ_block(&[
            "PATH=/usr/bin",
            "CLAUDE_CODE_SESSION_ID=43e78a1d-24a6-400f-8dc4-d4c9a70d9cc1",
            "TERM=xterm",
        ]);
        assert_eq!(
            parse_environ_var(&env, "CLAUDE_CODE_SESSION_ID").as_deref(),
            Some("43e78a1d-24a6-400f-8dc4-d4c9a70d9cc1")
        );
    }

    #[test]
    fn parse_environ_var_missing_key_is_none() {
        let env = environ_block(&["PATH=/usr/bin", "TERM=xterm"]);
        assert_eq!(parse_environ_var(&env, "CLAUDE_CODE_SESSION_ID"), None);
        // A prefix that is not a full key must not match.
        assert_eq!(parse_environ_var(&env, "PAT"), None);
    }

    #[test]
    fn parse_environ_var_distinguishes_two_processes_in_the_same_dir() {
        // Two Claude agents launched in the SAME directory: identical PWD,
        // different session IDs. The id must come from the process env, so
        // each is resolved independently and correctly.
        let agent_a = environ_block(&[
            "PWD=/Users/hanno/Projects/AppDock",
            "CLAUDE_CODE_SESSION_ID=aaaaaaaa-0000-0000-0000-000000000001",
        ]);
        let agent_b = environ_block(&[
            "PWD=/Users/hanno/Projects/AppDock",
            "CLAUDE_CODE_SESSION_ID=bbbbbbbb-0000-0000-0000-000000000002",
        ]);
        assert_eq!(
            parse_environ_var(&agent_a, "CLAUDE_CODE_SESSION_ID").as_deref(),
            Some("aaaaaaaa-0000-0000-0000-000000000001")
        );
        assert_eq!(
            parse_environ_var(&agent_b, "CLAUDE_CODE_SESSION_ID").as_deref(),
            Some("bbbbbbbb-0000-0000-0000-000000000002")
        );
        assert_ne!(
            parse_environ_var(&agent_a, "CLAUDE_CODE_SESSION_ID"),
            parse_environ_var(&agent_b, "CLAUDE_CODE_SESSION_ID"),
        );
    }

    #[test]
    fn read_limited_reader_returns_complete_data_under_limit() {
        let input = std::io::Cursor::new(b"image".to_vec());
        assert_eq!(
            read_limited_reader(input, 16).expect("limited read"),
            LimitedRead::Complete(b"image".to_vec())
        );
    }

    #[test]
    fn read_limited_reader_returns_empty_for_empty_input() {
        let input = std::io::Cursor::new(Vec::<u8>::new());
        assert_eq!(
            read_limited_reader(input, 16).expect("limited read"),
            LimitedRead::Empty
        );
    }

    #[test]
    fn read_limited_reader_accepts_data_exactly_at_limit() {
        let input = std::io::Cursor::new(b"four".to_vec());
        assert_eq!(
            read_limited_reader(input, 4).expect("limited read"),
            LimitedRead::Complete(b"four".to_vec())
        );
    }

    #[test]
    fn read_limited_reader_rejects_data_over_limit() {
        let input = std::io::Cursor::new(b"oversized".to_vec());
        assert_eq!(
            read_limited_reader(input, 4).expect("limited read"),
            LimitedRead::Oversized
        );
    }

    #[test]
    fn read_limited_reader_retries_interrupted_reads() {
        struct InterruptedOnce {
            interrupted: bool,
            inner: std::io::Cursor<Vec<u8>>,
        }

        impl std::io::Read for InterruptedOnce {
            fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
                if !self.interrupted {
                    self.interrupted = true;
                    return Err(std::io::ErrorKind::Interrupted.into());
                }
                self.inner.read(buffer)
            }
        }

        let input = InterruptedOnce {
            interrupted: false,
            inner: std::io::Cursor::new(b"image".to_vec()),
        };
        assert_eq!(
            read_limited_reader(input, 16).expect("limited read"),
            LimitedRead::Complete(b"image".to_vec())
        );
    }
}
