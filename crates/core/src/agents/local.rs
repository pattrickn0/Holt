//! Local-model CLIs (the model comes from the CLI or its configuration) and the colors of a new agent.

use std::path::Path;

use super::*;
/// Whether `kind` is a local model CLI whose active model is queried from the CLI/config (`ollama list`, Mimo config…).
pub fn is_local_cli(kind: &str) -> bool {
    matches!(kind, "ollama" | "mimo" | "llama-cli")
}

/// Curated dark-theme palette for user-added CLIs before falling back to golden-angle HSL generation.
const EXTRA_AGENT_COLORS: &[&str] = &[
    "#f472b6", "#fbbf24", "#60a5fa", "#c084fc", "#818cf8", "#4ade80", "#fb923c", "#f87171", "#38e8b0", "#facc15", "#a78bfa", "#67e8f9",
    "#f43f5e", "#84cc16", "#06b6d4", "#d946ef", "#fb7185", "#2dd4bf", "#34d399", "#e879f9", "#22d3ee", "#a3e635", "#38bdf8", "#f59e0b",
];

/// Generates a `#rrggbb` color for a newly added CLI that never repeats any color in [`AGENTS`] or `used`.
pub fn next_agent_color(used: &[&str]) -> String {
    let is_taken =
        |hex: &str| AGENTS.iter().any(|a| a.color.eq_ignore_ascii_case(hex)) || used.iter().any(|u| u.trim().eq_ignore_ascii_case(hex));
    for &candidate in EXTRA_AGENT_COLORS {
        if !is_taken(candidate) {
            return candidate.to_owned();
        }
    }
    for i in 0..10_000usize {
        let hue = ((i as f64) * 137.507_764_05) % 360.0;
        let hex = hsl_to_hex(hue, 0.78, 0.64);
        if !is_taken(&hex) {
            return hex;
        }
    }
    "#60a5fa".to_owned()
}

fn hsl_to_hex(h: f64, s: f64, l: f64) -> String {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;
    let (r1, g1, b1) = match h as u32 {
        0..=59 => (c, x, 0.0),
        60..=119 => (x, c, 0.0),
        120..=179 => (0.0, c, x),
        180..=239 => (0.0, x, c),
        240..=299 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let to_u8 = |v: f64| ((v + m).clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", to_u8(r1), to_u8(g1), to_u8(b1))
}

/// Splits a command line (`program + args`) into `(program, args)`, respecting single and double quotes
/// without mangling Windows backslashes.
pub fn split_command(command: &str) -> Option<(String, Vec<String>)> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_single = false;
    let mut in_double = false;
    let mut token_started = false;
    let mut chars = command.trim().chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\'' if !in_double => {
                in_single = !in_single;
                token_started = true;
            }
            '"' if !in_single => {
                in_double = !in_double;
                token_started = true;
            }
            '\\' if in_double && matches!(chars.peek(), Some('"' | '\\')) => {
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            }
            c if c.is_whitespace() && !in_single && !in_double => {
                if token_started || !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                    token_started = false;
                }
            }
            _ => {
                current.push(c);
                token_started = true;
            }
        }
    }
    if token_started || !current.is_empty() {
        tokens.push(current);
    }
    let mut iter = tokens.into_iter();
    let program = iter.next()?.trim().to_owned();
    if program.is_empty() {
        return None;
    }
    Some((program, iter.collect()))
}

/// Parses `ollama list` (or `ollama ps`) table output and returns the first model name in the `NAME` column.
pub fn parse_ollama_list(stdout: &str) -> Option<String> {
    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let first = trimmed.split_whitespace().next()?;
        if first.eq_ignore_ascii_case("NAME") || first.starts_with("Error") || first.starts_with("Warning") || first.starts_with("could") {
            continue;
        }
        if first.chars().next().is_some_and(|c| c.is_ascii_alphanumeric()) {
            return Some(first.to_owned());
        }
    }
    None
}

/// Extracts a model name from a Mimo config file (JSON, TOML, or plain model line) or CLI output.
pub fn parse_mimo_model(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) {
        for key in ["model", "defaultModel", "default_model", "activeModel", "active_model"] {
            if let Some(m) = v.get(key).and_then(serde_json::Value::as_str).map(str::trim).filter(|s| !s.is_empty()) {
                return Some(m.to_owned());
            }
        }
        return None;
    }
    for line in trimmed.lines() {
        let l = line.trim();
        if l.is_empty() || l.starts_with('#') || l.starts_with("//") {
            continue;
        }
        if let Some((k, v)) = l.split_once('=') {
            let key = k.trim();
            if matches!(key, "model" | "default_model" | "defaultModel" | "active_model") {
                let val = v.trim().trim_matches(['"', '\'']).trim();
                if !val.is_empty() && !val.contains(' ') {
                    return Some(val.to_owned());
                }
            }
            continue;
        }
        if let Some((k, v)) = l.split_once(':') {
            if k.trim().eq_ignore_ascii_case("model") {
                let val = v.trim().trim_matches(['"', '\'']).trim();
                if !val.is_empty() && !val.contains(' ') {
                    return Some(val.to_owned());
                }
            }
        }
    }
    // Single-line model identifier or `NAME ...` table.
    if !trimmed.contains('\n') && !trimmed.contains(' ') && trimmed.chars().next().is_some_and(|c| c.is_ascii_alphanumeric()) {
        let lower = trimmed.to_ascii_lowercase();
        if !lower.starts_with("usage") && !lower.starts_with("error") {
            return Some(trimmed.to_owned());
        }
    }
    parse_ollama_list(trimmed)
}

/// Extracts a model name from a custom CLI command string (`ollama run <m>`, `--model <m>`, `-m <m>`).
pub fn extract_command_model(command: &str) -> Option<String> {
    let (program, args) = split_command(command)?;
    let prog_stem = Path::new(&program).file_stem().and_then(|s| s.to_str()).unwrap_or(&program).to_ascii_lowercase();
    if prog_stem == "ollama" {
        let mut iter = args.iter();
        while let Some(a) = iter.next() {
            if a == "run" {
                return iter.next().filter(|m| !m.starts_with('-')).cloned();
            }
        }
    }
    let mut iter = args.iter();
    while let Some(a) = iter.next() {
        if let Some(val) = a.strip_prefix("--model=") {
            if !val.trim().is_empty() {
                return Some(val.trim().to_owned());
            }
        }
        if a == "--model" || a == "-m" {
            if let Some(val) = iter.next().map(|s| s.trim()).filter(|s| !s.is_empty() && !s.starts_with('-')) {
                // For `-m /path/to/model.gguf`, keep the filename stem so it reads cleanly in the menu/tab.
                if val.contains('/') || val.contains('\\') || val.ends_with(".gguf") {
                    if let Some(stem) = Path::new(val).file_stem().and_then(|s| s.to_str()).filter(|s| !s.is_empty()) {
                        return Some(stem.to_owned());
                    }
                }
                return Some(val.to_owned());
            }
        }
    }
    None
}
