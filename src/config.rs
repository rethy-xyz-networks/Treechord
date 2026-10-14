//! Configuration loading. Mirrors the keys and value forms of the AutoHotkey version.

use serde_json::Value;
use std::collections::HashMap;

/// A binding: `target` is an executable name or window class used to find an
/// existing window, `path` is what gets run when no such window exists.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Bind {
    pub target: String,
    pub path: String,
}

impl Bind {
    pub fn is_empty(&self) -> bool {
        self.target.is_empty() && self.path.is_empty()
    }
}

#[derive(Default)]
pub struct Config {
    pub binds: HashMap<String, Bind>,
}

/// Every key the configuration file may contain.
pub fn valid_keys() -> Vec<String> {
    let mut keys = Vec::new();
    for c in 'A'..='Z' {
        if !matches!(c, 'D' | 'R' | 'X') {
            keys.push(format!("bind{c}"));
        }
    }
    for c in 'A'..='Z' {
        keys.push(format!("bindShift{c}"));
    }
    for c in 'A'..='Z' {
        keys.push(format!("bindAlt{c}"));
    }
    keys.push("bindEnter".into());
    keys.push("bindShiftEnter".into());
    for n in 1..=5 {
        keys.push(format!("bindLaunch{n}"));
    }
    keys
}

/// `"path"` -> (file name, path); `["class-or-exe", "path"]` -> as given.
fn process_value(value: &Value) -> Result<Bind, ()> {
    match value {
        Value::Array(a) if a.len() == 2 => match (a[0].as_str(), a[1].as_str()) {
            (Some(t), Some(p)) => Ok(Bind { target: t.into(), path: p.into() }),
            _ => Err(()),
        },
        Value::String(s) => {
            let path = s.replace('/', "\\");
            let target = path.rsplit('\\').next().unwrap_or("").to_string();
            Ok(Bind { target, path })
        }
        _ => Err(()),
    }
}

pub struct Loaded {
    pub config: Config,
    /// `(key, value)` of every entry that was not understood.
    pub errors: Vec<(String, String)>,
}

pub fn load(path: &str) -> Result<Loaded, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    let text = text.trim_start_matches('\u{feff}');
    let json: Value = serde_json::from_str(text).map_err(|e| format!("{path}: {e}"))?;
    let obj = json.as_object().ok_or_else(|| format!("{path}: expected a JSON object"))?;

    let valid = valid_keys();
    let mut loaded = Loaded { config: Config::default(), errors: Vec::new() };
    for (key, value) in obj {
        if !valid.contains(key) {
            loaded.errors.push((key.clone(), value.to_string()));
            continue;
        }
        match process_value(value) {
            Ok(b) => {
                loaded.config.binds.insert(key.clone(), b);
            }
            Err(()) => loaded.errors.push((key.clone(), value.to_string())),
        }
    }
    Ok(loaded)
}

/// Writes the default configuration if `path` does not exist. Returns whether it was created.
pub fn init(path: &str) -> std::io::Result<bool> {
    if std::path::Path::new(path).exists() {
        return Ok(false);
    }
    let entries: Vec<String> = valid_keys()
        .into_iter()
        .map(|key| {
            let value = if key == "bindE" {
                "[ \"CabinetWClass\", \"C:/Windows/explorer.exe\" ]"
            } else {
                "\"\""
            };
            format!("\t\"{key}\": {value}")
        })
        .collect();
    std::fs::write(path, format!("{{\n\n{}\n\n}}\n", entries.join(",\n")))?;
    Ok(true)
}
