pub mod app;
pub mod favorites;
pub mod herdr;
pub mod ssh_config;
pub mod ui;

/// SSH aliases reach `ssh` as argv and Herdr `--env` values as `KEY=VALUE`.
/// Both are trust boundaries: `~/.ssh/config` is user input too.
pub fn valid_alias(alias: &str) -> bool {
    !alias.is_empty()
        && !alias.starts_with('-')
        && alias
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alias_validation_rejects_argv_and_shell_tricks() {
        for ok in ["dokku-prod", "web_prod", "nas.local", "a"] {
            assert!(valid_alias(ok), "{ok} should be accepted");
        }
        for bad in [
            "", "-oProxyCommand=x", "a b", "a;rm -rf /", "a$(id)", "a`id`",
            "a\nb", "a=b", "*", "a|b", "../x",
        ] {
            assert!(!valid_alias(bad), "{bad:?} should be rejected");
        }
    }
}
