//! Herdr spawns this as a pane entrypoint (argv, no shell), and it runs ssh as
//! a child, so nothing between the picker and OpenSSH ever sees a shell.

use std::os::unix::process::ExitStatusExt;
use std::process::{Command, ExitCode};

fn main() -> ExitCode {
    let Ok(alias) = std::env::var("HERDR_HOSTS_ALIAS") else {
        return fail("HERDR_HOSTS_ALIAS is not set");
    };
    if !herdr_hosts::valid_alias(&alias) {
        return fail(&format!("refusing suspicious ssh alias: {alias:?}"));
    }

    match Command::new("ssh").arg(&alias).status() {
        // 255 is ssh's own "something went wrong" code; anything else is the
        // remote command's status and means the session really ran.
        Ok(status) if status.code() == Some(255) => hold(ExitCode::from(255)),
        // No exit code at all means a signal killed ssh. Reporting that as a
        // clean exit would close the tab without a word, which is the very
        // thing hold() exists to prevent.
        Ok(status) if status.code().is_none() => {
            let signal = status.signal().unwrap_or(0);
            eprintln!("herdr-hosts: ssh was killed by signal {signal}");
            hold(ExitCode::FAILURE)
        }
        Ok(status) => ExitCode::from(status.code().unwrap_or(0) as u8),
        Err(e) => fail(&format!("cannot run ssh: {e}")),
    }
}

fn fail(message: &str) -> ExitCode {
    eprintln!("herdr-hosts: {message}");
    hold(ExitCode::FAILURE)
}

/// The pane closes the moment this process exits, so an unread error would
/// just flash past. Wait for the user before letting that happen.
fn hold(code: ExitCode) -> ExitCode {
    eprintln!("\n[herdr-hosts] ssh exited. Press Enter to close this pane.");
    let _ = std::io::stdin().read_line(&mut String::new());
    code
}
