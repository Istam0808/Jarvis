//! Legacy `command.yaml` format (`list` / `command.action` / `voice.phrases`).

use serde::Deserialize;

use super::structs::JCommand;

#[derive(Debug, Deserialize)]
struct LegacyFile {
    list: Vec<LegacyEntry>,
}

#[derive(Debug, Deserialize)]
struct LegacyEntry {
    command: LegacyCommand,
    #[serde(default)]
    voice: LegacyVoice,
    /// In shipped `command.yaml` files, phrases live next to `voice`, not under it.
    #[serde(default)]
    phrases: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct LegacyCommand {
    action: String,
    #[serde(default)]
    exe_path: String,
    #[serde(default)]
    exe_args: Vec<String>,
    #[serde(default)]
    cli_cmd: String,
    #[serde(default)]
    cli_args: Vec<String>,
    #[serde(default)]
    script: String,
    #[serde(default)]
    sandbox: String,
    #[serde(default)]
    timeout: Option<u64>,
}

#[derive(Debug, Default, Deserialize)]
struct LegacyVoice {
    #[serde(default)]
    sounds: Vec<String>,
    #[serde(default)]
    phrases: Vec<String>,
}

/// Parse legacy YAML text into [`JCommand`] list. `pack_id` is the folder name (e.g. `jarvis`).
pub fn commands_from_yaml_str(content: &str, pack_id: &str) -> Result<Vec<JCommand>, String> {
    let file: LegacyFile =
        serde_yaml::from_str(content).map_err(|e| format!("YAML parse error: {}", e))?;

    let mut out = Vec::with_capacity(file.list.len());

    for (idx, entry) in file.list.into_iter().enumerate() {
        let phrase_src = if !entry.phrases.is_empty() {
            entry.phrases
        } else {
            entry.voice.phrases
        };
        let phrases: Vec<String> = phrase_src
            .into_iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        if phrases.is_empty() {
            warn!(
                "Skipping legacy command index {} in pack '{}': no phrases",
                idx, pack_id
            );
            continue;
        }

        let id = format!("{}_cmd_{}", sanitize_id(pack_id), idx);
        let timeout = entry.command.timeout.unwrap_or(10_000);
        let sandbox = if entry.command.script.is_empty() {
            String::new()
        } else if entry.command.sandbox.is_empty() {
            "standard".to_string()
        } else {
            entry.command.sandbox.clone()
        };

        out.push(JCommand::from_legacy_ru(
            id,
            entry.command.action,
            entry.command.exe_path,
            entry.command.exe_args,
            entry.command.cli_cmd,
            entry.command.cli_args,
            entry.command.script,
            sandbox,
            timeout,
            phrases,
            entry.voice.sounds,
        ));
    }

    if out.is_empty() {
        return Err(format!(
            "Legacy YAML pack '{}' produced no commands (empty or missing phrases)",
            pack_id
        ));
    }

    Ok(out)
}

fn sanitize_id(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

