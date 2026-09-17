# herdr-hosts

A hierarchical SSH host picker for [Herdr](https://herdr.dev). Press a key,
find the host, press Enter — the session opens in a Herdr pane next to you.

```
┌────────────────────┬────────────────────────────────────┐
│ 1 │ dokku-prod │ nas │                                   │
│ nvim               │ $ ls                               │
│    ┌─────────────────────────────────┐                  │
│    │ SSH HOSTS                       │                  │
│    │                                 │                  │
│    │ ★ Favorites                     │                  │
│    │   ● dokku-prod ★                │                  │
│    │                                 │                  │
│    │ ▾ Personal                      │                  │
│    │   ▾ Hetzner                     │                  │
│    │     ● dokku-prod ★              │                  │
│    │     ● postgres                  │                  │
│    │ ▸ University                    │                  │
│    │ Ungrouped                       │                  │
│    │   ● nas                         │                  │
│    │ /prod▏                          │                  │
│    └─────────────────────────────────┘                  │
└────────────────────┴────────────────────────────────────┘
```

**Credentials are never stored by the plugin. SSH authentication is delegated
to OpenSSH.** No passwords, no keys, no vault. In fact the plugin writes
nothing at all except the list of hosts you starred.

## Prerequisites

* Herdr **0.9.0** or newer (`herdr --version`)
* OpenSSH client (`ssh` on `PATH`)
* Rust toolchain, for building from source
* Linux or macOS

## Install

```bash
git clone <this repo> herdr-hosts-plugin
cd herdr-hosts-plugin
cargo build --release
herdr plugin link "$PWD"
```

Open the picker:

```bash
herdr plugin action invoke herdr-hosts.open
```

Bind it to a free key in `~/.config/herdr/config.toml`, then run
`herdr server reload-config`:

```toml
[[keys.command]]
key = "prefix+shift+s"
type = "plugin_action"
command = "herdr-hosts.open"
description = "SSH hosts"
```

Check the key is free first — `herdr --default-config` lists Herdr's own
bindings, and your `config.toml` may add more. `prefix+s` is settings and
`prefix+h` is split_horizontal in the defaults.

## Keys

```
↑ / k     previous            /  search (alias, folder, note)
↓ / j     next                f  toggle favorite
→ / l     open folder, step in    r  reload config files
← / h     close folder, step out  ?  help
enter     connect, or return to an open session
shift+↵   always open a new tab for this host
space     expand / collapse   esc  leave search, then close
q         close
```

Each session opens in its own tab, named after the host, so it never shrinks
the pane you were working in, and Enter lands you in that tab.

Enter on a host that already has a session switches to that tab rather than
opening a second one. When you do want a second window onto the same host —
one tailing logs, one running commands — use **shift+enter**, which always
opens a new tab. Enter then returns to the oldest of them.

Shift+enter needs the kitty keyboard protocol, which Herdr supports. In a
terminal that does not, shift+enter behaves like plain Enter.

## Configuration

There is no config file. Everything comes from `~/.ssh/config`, including
`Include`d files:

```sshconfig
Include config.d/*

# --- Personal / Hetzner ---
Host dokku-prod   # main dokku box
    HostName 1.2.3.4
    User root

Host postgres

# === University ===
Host web-prod

Host *.internal
    User root
```

That renders as:

```
▾ Personal
  ▾ Hetzner
    ● dokku-prod  — main dokku box
    ● postgres
▾ University
  ● web-prod
```

**Folders** come from comment headings. A heading is a comment whose text is
fenced by at least two of the same separator — `# --- Work ---`,
`# === Work ===`, `# ** Work **` — and `/` nests it: `# --- Personal / Hetzner ---`.
An ordinary comment like `# remember to rotate this key` is *not* a heading, so
an existing config keeps its meaning. Hosts written above any heading land in
`Ungrouped`, which means a config with no headings at all is simply one flat
list.

**Notes** come from a trailing comment on the `Host` line. OpenSSH ignores
everything after `#` there (verified with `ssh -G`), so the note costs nothing.

**Patterns** such as `Host *.internal` declare defaults rather than a host you
connect to by name, so they are not listed.

OpenSSH has no notion of a group — `ssh_config(5)` uses the word only in cipher
names — so this convention is the plugin's, not OpenSSH's. Nothing here changes
how `ssh` itself reads the file.

**Favorites** are the one thing the plugin writes, and they go to its own state
directory (`herdr plugin config-dir herdr-hosts`, `state` sibling), one alias
per line. `~/.ssh/config` is never written to: it is how you reach every
machine you own, and a plugin has no business rewriting it.

## Architecture

The plugin is two binaries declared in `herdr-plugin.toml`:

* `herdr-hosts` — the picker, opened by Herdr as a **popup** (`placement =
  "popup"`). A popup leaves the tiled layout untouched and closes when the
  process exits.
* `herdr-hosts-ssh` — the launcher, opened in its **own tab** (`placement =
  "tab"`) with the chosen alias in `HERDR_HOSTS_ALIAS`. The tab is named after
  the host, so the tab bar doubles as a list of open sessions.

Herdr spawns plugin entrypoints as argv, and the launcher runs `ssh <alias>`
as argv, so **no shell is involved at any point** and there is nothing for a
hostile alias to interpolate into. Aliases are additionally restricted to
`[A-Za-z0-9._-]` and may not start with `-`.

```
src/
├── main.rs         event loop, key handling
├── ui.rs           rendering
├── app.rs          tree, navigation, search, favorites
├── ssh_config.rs   hosts, folders, notes, Include
├── favorites.rs    the starred list (the only thing written)
├── herdr.rs        every call into Herdr
└── bin/ssh.rs      HERDR_HOSTS_ALIAS → ssh
```

Only `herdr.rs` talks to Herdr, so everything else is testable without a
running server (`cargo test`).

## Limitations

* No online/offline detection. ICMP is unreliable for this and a TCP probe per
  host is not worth the first release.
* Folders and notes are written in `~/.ssh/config`, not in the UI.
* No file watcher — press `r` after editing `~/.ssh/config`.
* The picker does not resolve `HostName`/`User`/`Port`. If you need those, ask
  OpenSSH: `ssh -G <alias>`.
* Linux and macOS only.
* A persistent left-docked sidebar is not implemented; see `SPEC.md` §21.

## Credits

Not derived from its code, but the plugin-pane approach was informed by
[`alexarthurs/herdr-sidebar`](https://github.com/alexarthurs/herdr-sidebar)
(MIT).
