//! Regenerates docs/preview.png.
//!
//! Renders the real picker over a made-up config — nothing from the machine
//! running it reaches the screenshot — writes an HTML mirror of the terminal
//! cells, then:
//!
//! ```sh
//! cargo run --release --example preview
//! chromium --headless --disable-gpu --hide-scrollbars \
//!   --force-device-scale-factor=2 --default-background-color=00000000 \
//!   --screenshot=/tmp/shot.png --window-size=600,620 file:///tmp/preview.html
//! magick /tmp/shot.png -bordercolor none -trim +repage -border 14 docs/preview.png
//! ```
use herdr_hosts::app::App;
use herdr_hosts::ui;
use ratatui::backend::TestBackend;
use ratatui::widgets::ListState;
use ratatui::Terminal;

const CONFIG: &str = "\
Host laptop

# --- Personal / Hetzner ---
Host dokku-prod   # main production box
Host postgres
Host backup

# --- Personal / Home ---
Host nas

# === University / Research ===
Host research-vm
Host gpu-box      # shared, check the queue first

# === University / Production ===
Host web-prod
Host gitlab
";

fn main() {
    let root = std::env::temp_dir().join("herdr-hosts-preview");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join(".ssh")).unwrap();
    std::fs::create_dir_all(root.join("state")).unwrap();
    std::fs::write(root.join(".ssh/config"), CONFIG).unwrap();
    std::fs::write(root.join("state/favorites"), "dokku-prod\nresearch-vm\n").unwrap();
    std::env::set_var("HOME", &root);
    std::env::set_var("HERDR_PLUGIN_STATE_DIR", root.join("state"));

    let (w, h) = (54u16, 21u16);
    let mut app = App::new();
    app.selected = app
        .rows
        .iter()
        .position(|r| r.alias() == Some("postgres"))
        .unwrap();

    let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
    let mut state = ListState::default();
    terminal
        .draw(|frame| ui::draw(frame, &mut app, &mut state))
        .unwrap();

    let buffer = terminal.backend().buffer();
    let mut lines = Vec::new();
    for y in 0..h {
        let mut line = String::new();
        for x in 0..w {
            let cell = &buffer[(x, y)];
            let modifier = format!("{:?}", cell.modifier);
            let mut classes = Vec::new();
            if modifier.contains("REVERSED") {
                classes.push("rev");
            }
            if modifier.contains("BOLD") {
                classes.push("b");
            }
            if modifier.contains("DIM") {
                classes.push("d");
            }
            let symbol = match cell.symbol() {
                "&" => "&amp;".into(),
                "<" => "&lt;".into(),
                ">" => "&gt;".into(),
                "" => " ".to_string(),
                other => other.to_string(),
            };
            if classes.is_empty() {
                line.push_str(&symbol);
            } else {
                line.push_str(&format!("<span class=\"{}\">{symbol}</span>", classes.join(" ")));
            }
        }
        lines.push(line);
    }

    let html = format!(
        r#"<!doctype html><meta charset="utf-8"><style>
*{{margin:0;padding:0;box-sizing:border-box}}
body{{background:transparent;padding:26px;display:inline-block}}
.win{{background:#12141c;border:1px solid #2a2f45;border-radius:10px;
     box-shadow:0 18px 50px rgba(0,0,0,.55);overflow:hidden}}
.bar{{padding:9px 14px;font:600 12.5px/1 'JetBrainsMono Nerd Font',monospace;
     color:#6b7394;border-bottom:1px solid #2a2f45;letter-spacing:.4px}}
pre{{margin:0;padding:12px 14px 14px;color:#c8d0e0;
    font:14px/1.5 'JetBrainsMono Nerd Font',monospace;white-space:pre}}
.b{{font-weight:700;color:#e6ebf7}}
.d{{color:#6b7394}}
.rev{{background:#7aa2f7;color:#12141c;font-weight:600}}
</style><div class="win"><div class="bar">SSH Hosts</div><pre>{}</pre></div>"#,
        lines.join("\n")
    );

    let out = std::env::temp_dir().join("preview.html");
    std::fs::write(&out, html).unwrap();
    eprintln!("wrote {}", out.display());
}
