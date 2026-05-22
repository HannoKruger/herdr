//! Resolving the Claude Code session id for a pane running `claude`.
//!
//! herdr records this while a pane's `claude` process is alive so a restored
//! session can bring the conversation back with `claude --resume <id>`.
//!
//! The id is read from the process command line — `claude --resume <id>` or
//! `claude --session-id <id>`. That is exact and never wrong, and it tells
//! several `claude` processes apart even in the same directory. A bare
//! `claude` carries no id, so it is simply left alone (no risky guessing).

/// Whether `s` is shaped like a session UUID (`8-4-4-4-12` hex).
pub fn looks_like_uuid(s: &str) -> bool {
    s.len() == 36
        && s.bytes().enumerate().all(|(i, b)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
}

/// Pull an explicitly pinned session id out of a `claude` command line:
/// `--resume <id>`, `--session-id <id>`, or `-r <id>`.
pub fn session_from_cmdline(cmdline: &str) -> Option<String> {
    let args: Vec<&str> = cmdline.split_whitespace().collect();
    args.iter().enumerate().find_map(|(i, arg)| {
        if !matches!(*arg, "--resume" | "--session-id" | "-r") {
            return None;
        }
        args.get(i + 1)
            .filter(|id| looks_like_uuid(id))
            .map(|id| id.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const UUID_A: &str = "43e78a1d-24a6-400f-8dc4-d4c9a70d9cc1";
    const UUID_B: &str = "9c09f191-c0f7-4af7-88fd-ba089767f6a7";

    #[test]
    fn looks_like_uuid_accepts_only_uuid_shape() {
        assert!(looks_like_uuid(UUID_A));
        assert!(!looks_like_uuid("not-a-uuid"));
        assert!(!looks_like_uuid("43e78a1d24a6400f8dc4d4c9a70d9cc1"));
        assert!(!looks_like_uuid(""));
    }

    #[test]
    fn session_from_cmdline_reads_explicit_flags() {
        assert_eq!(
            session_from_cmdline(&format!("claude --resume {UUID_A}")).as_deref(),
            Some(UUID_A)
        );
        assert_eq!(
            session_from_cmdline(&format!("claude --session-id {UUID_B} --model opus")).as_deref(),
            Some(UUID_B)
        );
        assert_eq!(
            session_from_cmdline(&format!("node /opt/bin/claude.js -r {UUID_A}")).as_deref(),
            Some(UUID_A)
        );
        // Two same-directory agents are told apart purely by their own argv.
        assert_ne!(
            session_from_cmdline(&format!("claude --resume {UUID_A}")),
            session_from_cmdline(&format!("claude --resume {UUID_B}")),
        );
    }

    #[test]
    fn session_from_cmdline_ignores_plain_or_invalid() {
        assert_eq!(session_from_cmdline("claude"), None);
        assert_eq!(session_from_cmdline("claude --continue"), None);
        assert_eq!(session_from_cmdline("claude --resume not-a-uuid"), None);
        assert_eq!(session_from_cmdline("claude --resume"), None);
    }
}
