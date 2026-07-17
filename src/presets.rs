//! Transparent preset and built-in macro registry (D12 upgrade path).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{CliError, Diag};

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preset {
    pub name: String,
    pub command: String,
    pub body: serde_json::Value,
    pub source: String,
}

#[derive(Debug, Default, Deserialize)]
struct PresetFile {
    #[serde(default)]
    presets: BTreeMap<String, RawPreset>,
}

#[derive(Debug, Deserialize)]
struct RawPreset {
    #[serde(default)]
    command: Option<String>,
    #[serde(default)]
    body: toml::Table,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MacroDef {
    pub name: &'static str,
    pub expands_to: &'static str,
    pub description: &'static str,
}

pub const MACROS: &[MacroDef] = &[
    MacroDef {
        name: "ask",
        expands_to: "answer QUESTION",
        description: "One-shot cited answer.",
    },
    MacroDef {
        name: "fetch",
        expands_to: "contents URL... --text --summary-query 'Summarize the page'",
        description: "Fetch and summarize known pages.",
    },
];

pub fn user_presets_path() -> PathBuf {
    if let Some(path) = nonempty_env("EXA_AGENT_PRESETS") {
        return PathBuf::from(path);
    }
    if let Some(xdg) = nonempty_env("XDG_CONFIG_HOME") {
        return PathBuf::from(xdg).join("exa-agent").join("presets.toml");
    }
    std::env::var("HOME")
        .map(|home| {
            PathBuf::from(home)
                .join(".config")
                .join("exa-agent")
                .join("presets.toml")
        })
        .unwrap_or_else(|_| PathBuf::from(".config/exa-agent/presets.toml"))
}

pub fn local_presets_path() -> PathBuf {
    if let Some(path) = nonempty_env("EXA_AGENT_LOCAL_PRESETS") {
        return PathBuf::from(path);
    }
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let root = cwd
        .ancestors()
        .find(|path| path.join(".git").exists())
        .unwrap_or(&cwd);
    root.join(".exa-agent").join("presets.toml")
}

pub fn load_presets() -> Result<BTreeMap<String, Preset>, CliError> {
    let mut presets = BTreeMap::new();
    load_file(&user_presets_path(), &mut presets)?;
    load_file(&local_presets_path(), &mut presets)?;
    Ok(presets)
}

pub fn get_preset(name: &str, command: &str) -> Result<Preset, CliError> {
    let preset = find_preset(name)?;
    if preset.command != command {
        return Err(CliError::Config(
            Diag::new(
                "config_invalid",
                format!(
                    "preset `{name}` targets `{}`, not `{command}`",
                    preset.command
                ),
            )
            .with_suggestion(format!("exa-agent {} --help", preset.command)),
        ));
    }
    Ok(preset)
}

pub fn find_preset(name: &str) -> Result<Preset, CliError> {
    load_presets()?.remove(name).ok_or_else(|| {
        CliError::Config(
            Diag::new("config_invalid", format!("unknown preset `{name}`"))
                .with_suggestion("exa-agent preset list"),
        )
    })
}

pub fn get_macro(name: &str) -> Result<MacroDef, CliError> {
    MACROS
        .iter()
        .copied()
        .find(|item| item.name == name)
        .ok_or_else(|| {
            CliError::Usage(
                Diag::new("invalid_value", format!("unknown macro `{name}`"))
                    .with_details(serde_json::json!({
                        "valid": MACROS.iter().map(|item| item.name).collect::<Vec<_>>()
                    }))
                    .with_suggestion("exa-agent macro list"),
            )
        })
}

fn load_file(path: &Path, target: &mut BTreeMap<String, Preset>) -> Result<(), CliError> {
    if !path.exists() {
        return Ok(());
    }
    let raw = fs::read_to_string(path).map_err(|error| preset_error(path, error))?;
    let file: PresetFile = toml::from_str(&raw).map_err(|error| preset_error(path, error))?;
    for (name, raw) in file.presets {
        let overlay = serde_json::to_value(raw.body).map_err(|error| preset_error(path, error))?;
        let previous = target.remove(&name);
        let command = raw
            .command
            .or_else(|| previous.as_ref().map(|preset| preset.command.clone()))
            .ok_or_else(|| {
                preset_error(
                    path,
                    format!("preset `{name}` must define `command` in at least one layer"),
                )
            })?;
        let mut body = previous
            .map(|preset| preset.body)
            .unwrap_or_else(|| serde_json::json!({}));
        crate::request::deep_merge(&mut body, overlay);
        target.insert(
            name.clone(),
            Preset {
                name,
                command,
                body,
                source: path.display().to_string(),
            },
        );
    }
    Ok(())
}

fn preset_error(path: &Path, error: impl std::fmt::Display) -> CliError {
    CliError::Config(
        Diag::new(
            "config_parse_error",
            format!("failed to load presets at {}: {error}", path.display()),
        )
        .with_suggestion(format!("edit {}", path.display())),
    )
}

fn nonempty_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
}
