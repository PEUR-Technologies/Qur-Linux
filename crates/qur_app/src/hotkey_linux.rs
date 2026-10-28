//! `[keys] global` on Linux: Wayland gives apps no global keys, so on Hyprland
//! Qur registers a compositor bind that brings Qur's window to the current
//! workspace, or hides it (a special workspace) when it already has focus.
//! Mac's Cmd is Super here. Other desktops: bind it in their own shortcut settings.
//! shortcut: the bind is runtime-only (`hyprctl reload` drops it until Qur restarts or Settings saves); use the GlobalShortcuts portal if other desktops need it.

use std::process::{Command, Stdio};
use std::sync::Mutex;

/// The bind registered last ("MODS,key"), to drop it when the key changes.
static BOUND: Mutex<Option<String>> = Mutex::new(None);

const TOGGLE: &str = "if hyprctl activewindow | grep -qx '\tclass: qur'; then hyprctl dispatch movetoworkspacesilent special:qur,class:^qur$; \
else hyprctl dispatch movetoworkspace \"$(hyprctl activeworkspace | awk 'NR==1{print $3}')\",class:^qur$; fi";

fn hyprctl(args: &[&str]) -> bool {
    Command::new("hyprctl").args(args).stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|s| s.success())
}

/// Register `keys` (GPUI syntax, e.g. "cmd-\\"), replacing any earlier one; "" turns it off.
pub fn set(keys: &str) -> Result<(), String> {
    if std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_none() {
        return Ok(());
    }
    let mut bound = BOUND.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(old) = bound.take() {
        hyprctl(&["keyword", "unbind", &old]);
    }
    if keys.trim().is_empty() {
        return Ok(());
    }
    let bind = parse(keys).ok_or_else(|| format!("[keys] global: can't use {keys:?}"))?;
    if !hyprctl(&["keyword", "bind", &format!("{bind},exec,sh -c \"{}\"", TOGGLE.replace('"', "\\\""))]) {
        return Err(format!("[keys] global: Hyprland refused {keys:?}"));
    }
    *bound = Some(bind);
    Ok(())
}

/// GPUI-style keys to Hyprland's "MODS,keysym".
fn parse(keys: &str) -> Option<String> {
    let mut parts: Vec<&str> = keys.trim().split('-').collect();
    let key = if keys.ends_with("--") {
        parts.truncate(parts.len() - 2);
        "-"
    } else {
        parts.pop()?
    };
    let mods = parts
        .iter()
        .map(|m| match *m {
            "cmd" | "super" => Some("SUPER"),
            "shift" => Some("SHIFT"),
            "alt" | "opt" => Some("ALT"),
            "ctrl" => Some("CTRL"),
            _ => None,
        })
        .collect::<Option<Vec<_>>>()?;
    let f = key.strip_prefix('f').and_then(|n| n.parse::<u32>().ok()).filter(|n| (1..=20).contains(n));
    if mods.is_empty() && f.is_none() {
        return None;
    }
    let sym = match key {
        "\\" => "backslash",
        "`" => "grave",
        "-" => "minus",
        "=" => "equal",
        "[" => "bracketleft",
        "]" => "bracketright",
        ";" => "semicolon",
        "'" => "apostrophe",
        "," => "comma",
        "." => "period",
        "/" => "slash",
        "enter" => "Return",
        "tab" => "Tab",
        "escape" => "Escape",
        "left" => "Left",
        "right" => "Right",
        "up" => "Up",
        "down" => "Down",
        _ if f.is_some() => return Some(format!("{},F{}", mods.join(" "), f?)),
        k if k.len() == 1 && k.chars().all(|c| c.is_ascii_alphanumeric()) => k,
        "space" => "space",
        _ => return None,
    };
    Some(format!("{},{sym}", mods.join(" ")))
}

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn parses_gpui_keys() {
        assert_eq!(parse("cmd-\\").as_deref(), Some("SUPER,backslash"));
        assert_eq!(parse("ctrl-shift-space").as_deref(), Some("CTRL SHIFT,space"));
        assert_eq!(parse("alt-q").as_deref(), Some("ALT,q"));
        assert_eq!(parse("cmd--").as_deref(), Some("SUPER,minus"));
        assert_eq!(parse("f13").as_deref(), Some(",F13"));
        assert_eq!(parse("q"), None);
        assert_eq!(parse("ctrl-pageup"), None);
    }
}
