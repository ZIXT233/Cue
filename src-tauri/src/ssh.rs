//! Compatibility surface over [`crate::remote`].
//!
//! This module used to be the SSH implementation: it located `ssh.exe`, built
//! its argument list, ran it under an askpass helper and cached passwords
//! because Win32-OpenSSH has no ControlMaster. All of that now lives in the
//! protocol layer, so what remains is the small vocabulary the rest of the
//! backend already speaks: quoting helpers, a login-shell wrapper, and the
//! one-shot exec calls that harness probing and remote browsing are built on.

use crate::error::{AppError, AppResult};
use crate::models::RemoteHost;

/// Whether Que currently holds an authenticated connection to `host`.
pub async fn is_connected(host: &str) -> bool {
    crate::remote::is_connected(host).await
}

/// Establish (or reuse) the connection for a host, so later commands and
/// terminals do not have to authenticate again.
pub async fn connect_host(host: &str, password: Option<String>, trusted_prompt: Option<String>) -> AppResult<()> {
    crate::remote::session_with(host, password, trusted_prompt).await.map(|_| ())
}

/// Validate a host that is not saved yet (the editor's "test" action).
pub async fn test_target(target: RemoteHost, password: Option<String>, trusted_prompt: Option<String>) -> AppResult<()> {
    let target = crate::remote::Target::from_host(&target);
    crate::remote::connect_target(&target, password, trusted_prompt).await.map(|_| ())
}

/// POSIX single-quote a value for a remote shell.
pub fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r#"'"'"'"#))
}

/// Run a command through the user's interactive login shell.
///
/// `exec` on an SSH channel runs the command with `$SHELL -c`, which reads no
/// rc files, so anything that needs the user's PATH or aliases has to say so
/// explicitly.
pub fn ssh_login_command(command: &str) -> String {
    let run = format!("exec /bin/sh -c {}", shell_quote(command));
    format!("/bin/sh -c {}", shell_quote(&format!("exec \"${{SHELL:-/bin/sh}}\" -ilc {}", shell_quote(&run))))
}

/// The one-shot "is this CLI on the host's PATH" probe a remote launch runs before
/// opening a card.
///
/// `command -v` exits non-zero when the name is absent, and a non-zero exit would
/// surface the login shell's rc noise (ioctl complaints from a pty-less shell) as the
/// error text. The trailing `true` keeps the probe itself alive, so an empty answer is
/// the "not installed" verdict and the caller gets to write the message.
pub fn ssh_cli_probe(executable: &str) -> String {
    format!("command -v {}; true", shell_quote(executable))
}

/// An interactive login shell on the remote host, started in `directory`.
///
/// `exec` on an SSH channel runs the command through `$SHELL -c`, which reads no
/// rc files and is not interactive; replacing that shell with the user's login
/// shell is what gives the pty a prompt, job control and the user's PATH. The
/// channel already owns a pty, so nothing local has to imitate one.
pub fn remote_login_shell(directory: &str, dark: bool) -> String {
    format!(
        "cd {} && export COLORFGBG={} COLORTERM=truecolor && exec \"${{SHELL:-/bin/sh}}\" -il",
        shell_quote(directory),
        shell_quote(crate::terminal_theme::colorfgbg(dark)),
    )
}

pub async fn ssh_exec(host: &str, command: &str) -> AppResult<Vec<u8>> {
    ssh_exec_stdin(host, command, &[]).await
}

pub async fn ssh_exec_stdin(host: &str, command: &str, stdin: &[u8]) -> AppResult<Vec<u8>> {
    crate::debuglog::debug("ssh", &format!("exec host={host} cmd={:?}", crate::debuglog::clip(command, 500)));
    let output = crate::remote::exec(host, command, stdin).await?;
    crate::debuglog::debug("ssh", &format!("exec ok stdout_len={}", output.len()));
    Ok(output)
}

/// Run a command through the login shell and return only its output.
///
/// A login shell may print a banner or MOTD before the command runs; the marker
/// lets us drop everything up to the real output.
pub async fn ssh_login_exec(host: &str, command: &str) -> AppResult<Vec<u8>> {
    let marker = format!("__QUE_LOGIN_{}__", uuid::Uuid::new_v4());
    let wrapped = ssh_login_command(&format!("printf '%s' {}; {}", shell_quote(&marker), command));
    let output = ssh_exec(host, &wrapped).await?;
    let start = output.windows(marker.len()).position(|w| w == marker.as_bytes()).ok_or_else(|| AppError::machine("REMOTE_SHELL_NO_OUTPUT"))?;
    Ok(output[start + marker.len()..].to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The string a remote pty runs, byte for byte. It has to end in the user's
    /// *login* shell: `exec` on a channel otherwise runs `$SHELL -c` with no rc
    /// files, which is the whole reason a bare `exec $SHELL` is not enough.
    #[test]
    fn a_remote_login_shell_starts_in_the_workspace() {
        assert_eq!(remote_login_shell("/srv/app", false), r#"cd '/srv/app' && export COLORFGBG='0;15' COLORTERM=truecolor && exec "${SHELL:-/bin/sh}" -il"#);
    }

    /// A directory with a space or a quote in it must stay one shell word, or the
    /// rest of it becomes a command.
    #[test]
    fn a_remote_directory_stays_one_shell_word() {
        assert_eq!(shell_quote("/srv/my app/it's here"), r#"'/srv/my app/it'"'"'s here'"#);
        assert_eq!(remote_login_shell("/srv/my app/it's here", false), r#"cd '/srv/my app/it'"'"'s here' && export COLORFGBG='0;15' COLORTERM=truecolor && exec "${SHELL:-/bin/sh}" -il"#);
        assert_eq!(remote_login_shell("~/notes", false), r#"cd '~/notes' && export COLORFGBG='0;15' COLORTERM=truecolor && exec "${SHELL:-/bin/sh}" -il"#);
    }

    /// `ssh_login_command` is the other half of the pair: harness CLI launches need
    /// `-c` and a quoted command, side terminals need the interactive flags.
    #[test]
    fn a_login_command_runs_one_command_and_a_login_shell_stays_interactive() {
        let command = ssh_login_command("codex --version");
        assert!(command.contains(r#"-ilc"#), "{command}");
        assert!(command.contains(r#"'codex --version'"#), "{command}");
        assert!(!remote_login_shell("/srv/app", false).contains("-c "));
    }

    /// The probe must always exit zero — a non-zero exit would carry the shell's rc
    /// noise back as the error text — and the CLI name must stay one shell word.
    #[test]
    fn a_cli_probe_always_succeeds_and_keeps_the_name_quoted() {
        assert_eq!(ssh_cli_probe("codex"), "command -v 'codex'; true");
        assert_eq!(ssh_cli_probe("my agent"), "command -v 'my agent'; true");
    }
}
