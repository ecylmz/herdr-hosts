//! Every call into Herdr lives here. The rest of the plugin is ordinary data
//! handling and can be tested without a running server.

use std::process::Command;

fn bin() -> String {
    std::env::var("HERDR_BIN_PATH").unwrap_or_else(|_| "herdr".into())
}

fn plugin_id() -> String {
    std::env::var("HERDR_PLUGIN_ID").unwrap_or_else(|_| "herdr-hosts".into())
}

pub fn inside_herdr() -> bool {
    std::env::var("HERDR_ENV").as_deref() == Ok("1")
}

/// Pane and tab ids are opaque (`w1:p1`, `w1:t1`) but go back out as argv, so
/// reject anything that could be read as a flag.
fn valid_id(id: &str) -> Option<String> {
    let ok = !id.is_empty()
        && !id.starts_with('-')
        && id.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, ':' | '_' | '-'));
    ok.then(|| id.to_string())
}

fn opened_pane_field(json: &str, field: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    valid_id(v["result"]["plugin_pane"]["pane"][field].as_str()?)
}

pub fn pane_id_from_open_response(json: &str) -> Option<String> {
    opened_pane_field(json, "pane_id")
}

pub fn tab_id_from_open_response(json: &str) -> Option<String> {
    opened_pane_field(json, "tab_id")
}

/// Pane label set by `open_ssh`, and the key `find_ssh_pane` looks for.
fn pane_tag(alias: &str) -> String {
    format!("ssh:{alias}")
}

pub fn pane_id_with_tag(json: &str, alias: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    let tag = pane_tag(alias);
    v["result"]["panes"]
        .as_array()?
        .iter()
        .find(|p| p["label"].as_str() == Some(&tag))
        .and_then(|p| valid_id(p["pane_id"].as_str()?))
}

fn run(args: &[&str]) -> Result<String, String> {
    let out = Command::new(bin())
        .args(args)
        .output()
        .map_err(|e| format!("herdr: {e}"))?;
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let message = if stderr.trim().is_empty() { stdout.trim() } else { stderr.trim() };
        return Err(format!("herdr: {message}"));
    }
    // The CLI answers 0 with an {"error": ...} body for API-level refusals
    // such as ui_busy, so success is not enough on its own.
    if let Some(code) = error_code(&stdout) {
        return Err(format!("herdr: {code}"));
    }
    Ok(stdout)
}

fn error_code(json: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    let error = v.get("error")?;
    Some(
        error["message"]
            .as_str()
            .or_else(|| error["code"].as_str())?
            .to_string(),
    )
}

/// Open the picker popup. Used by the plugin action behind the keybinding.
pub fn open_picker() -> Result<(), String> {
    run(&["plugin", "pane", "open", "--plugin", &plugin_id(), "--entrypoint", "picker"]).map(|_| ())
}

/// An already-open session for this alias, if one is still alive.
pub fn find_ssh_pane(alias: &str) -> Option<String> {
    pane_id_with_tag(&run(&["pane", "list"]).ok()?, alias)
}

/// Focus an existing session. The pane carries its tab with it, but the tab is
/// focused explicitly for the same reason as in `open_ssh`.
pub fn focus_pane(pane_id: &str) -> Result<(), String> {
    let json = run(&["plugin", "pane", "focus", pane_id])?;
    if let Some(tab) = tab_id_from_open_response(&json) {
        let _ = run(&["tab", "focus", &tab]);
    }
    Ok(())
}

/// Open a tab running `ssh <alias>`, argv all the way: Herdr spawns the
/// launcher entrypoint, the launcher runs ssh. A session gets its own tab
/// rather than a split, so it never shrinks the pane you were working in.
pub fn open_ssh(alias: &str) -> Result<String, String> {
    if !crate::valid_alias(alias) {
        return Err(format!("invalid ssh alias: {alias}"));
    }
    let plugin = plugin_id();
    let env = format!("HERDR_HOSTS_ALIAS={alias}");
    let json = run(&[
        "plugin", "pane", "open",
        "--plugin", &plugin,
        "--entrypoint", "ssh",
        "--placement", "tab",
        "--env", &env,
        "--focus",
    ])?;

    let pane = pane_id_from_open_response(&json).ok_or("herdr did not return a pane id")?;

    // Best effort from here on: the pane tag is what lets a later Enter focus
    // this session instead of opening a second one, and the tab label is how
    // you find it in the tab bar. Neither is worth failing the connect.
    let _ = run(&["pane", "rename", &pane, &pane_tag(alias)]);
    if let Some(tab) = tab_id_from_open_response(&json) {
        let _ = run(&["tab", "rename", &tab, alias]);
        // `--focus` above already focuses the new pane, but focus the tab
        // explicitly too: the picker is a modal popup closing in the same
        // breath, and landing on the session is the whole point of pressing
        // enter.
        let _ = run(&["tab", "focus", &tab]);
    }
    Ok(pane)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pane_and_tab_ids_are_read_from_open_response() {
        let json = r#"{"result":{"plugin_pane":{"entrypoint":"ssh",
            "pane":{"pane_id":"wR:p6","tab_id":"wR:t3","workspace_id":"wR"}}}}"#;
        assert_eq!(pane_id_from_open_response(json), Some("wR:p6".into()));
        assert_eq!(tab_id_from_open_response(json), Some("wR:t3".into()));

        assert_eq!(pane_id_from_open_response(r#"{"error":{"code":"ui_busy"}}"#), None);
        assert_eq!(pane_id_from_open_response("not json"), None);
        // A tab label rename must never be talked into taking a flag.
        let evil = r#"{"result":{"plugin_pane":{"pane":{"pane_id":"w1:p1","tab_id":"--evil"}}}}"#;
        assert_eq!(tab_id_from_open_response(evil), None);
    }

    #[test]
    fn existing_session_is_found_by_its_tag() {
        let json = r#"{"result":{"panes":[
            {"pane_id":"wN:p1","label":null},
            {"pane_id":"wN:p2","label":"ssh:dokku-prod"},
            {"pane_id":"wN:p3","label":"ssh:postgres"}
        ]}}"#;
        assert_eq!(pane_id_with_tag(json, "dokku-prod"), Some("wN:p2".into()));
        assert_eq!(pane_id_with_tag(json, "postgres"), Some("wN:p3".into()));
        assert_eq!(pane_id_with_tag(json, "nas"), None);
    }

    #[test]
    fn api_refusals_are_errors_even_when_the_cli_exits_zero() {
        assert_eq!(
            error_code(r#"{"error":{"code":"ui_busy","message":"a modal is open"}}"#),
            Some("a modal is open".into())
        );
        assert_eq!(error_code(r#"{"result":{"type":"ok"}}"#), None);
        assert_eq!(error_code("not json"), None);
    }
}
