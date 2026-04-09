use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::fs;
use std::time::Duration;
use std::process::{Child, Command};

use seqdiff::ratio;

mod structs;
mod yaml_legacy;
pub use structs::*;

use crate::{config, i18n, APP_DIR};

#[cfg(feature = "lua")]
use crate::lua::{self, SandboxLevel, CommandContext};

pub fn parse_commands() -> Result<Vec<JCommandsList>, String> {
    let mut commands: Vec<JCommandsList> = Vec::new();

    let commands_path = APP_DIR.join(config::COMMANDS_PATH);
    let cmd_dirs = fs::read_dir(&commands_path)
        .map_err(|e| format!("Error reading commands directory {:?}: {}", commands_path, e))?;

    for entry in cmd_dirs.flatten() {
        let cmd_path = entry.path();
        let toml_file = cmd_path.join("command.toml");
        let yaml_file = cmd_path.join("command.yaml");

        let pack_name = entry
            .file_name()
            .into_string()
            .unwrap_or_else(|s| s.to_string_lossy().into_owned());

        if toml_file.exists() {
            let content = match fs::read_to_string(&toml_file) {
                Ok(c) => c,
                Err(e) => {
                    warn!("Failed to read {}: {}", toml_file.display(), e);
                    continue;
                }
            };

            let file: JCommandsList = match toml::from_str(&content) {
                Ok(f) => f,
                Err(e) => {
                    warn!("Failed to parse {}: {}", toml_file.display(), e);
                    continue;
                }
            };

            commands.push(JCommandsList {
                path: cmd_path,
                commands: file.commands,
            });
            continue;
        }

        if yaml_file.exists() {
            let content = match fs::read_to_string(&yaml_file) {
                Ok(c) => c,
                Err(e) => {
                    warn!("Failed to read {}: {}", yaml_file.display(), e);
                    continue;
                }
            };

            match yaml_legacy::commands_from_yaml_str(&content, &pack_name) {
                Ok(cmds) => {
                    commands.push(JCommandsList {
                        path: cmd_path,
                        commands: cmds,
                    });
                }
                Err(e) => {
                    warn!("Failed to parse legacy {}: {}", yaml_file.display(), e);
                }
            }
        }
    }

    if commands.is_empty() {
        Err("No commands found".into())
    } else {
        info!("Loaded {} command pack(s)", commands.len());
        Ok(commands)
    }
}


pub fn commands_hash(commands: &[JCommandsList]) -> String {
    use sha2::{Sha256, Digest};
    
    let mut hasher = Sha256::new();
    
    let lang = i18n::get_language();
    hasher.update(lang.as_bytes());
    hasher.update(b"|");

    // collect all command ids and phrases for current language, sorted
    let mut all_data: Vec<(&str, _)> = commands.iter()
        .flat_map(|ac| ac.commands.iter().map(|c| (c.id.as_str(), c.get_phrases(&lang))))
        .collect();
    all_data.sort_by_key(|(id, _)| *id);
    
    for (id, phrases) in all_data {
        hasher.update(id.as_bytes());
        for phrase in phrases.iter() {
            hasher.update(phrase.as_bytes());
        }
    }
    
    format!("{:x}", hasher.finalize())
}


const RU_NUM_WORDS: &[&str] = &[
    "ноль", "один", "одна", "два", "две", "три", "четыре", "пять", "шесть", "семь", "восемь",
    "девять", "десять", "одиннадцать", "двенадцать", "тринадцать", "четырнадцать", "пятнадцать",
    "шестнадцать", "семнадцать", "восемнадцать", "девятнадцать", "двадцать", "тридцать", "сорок",
    "пятьдесят", "шестьдесят", "семьдесят", "восемьдесят", "девяносто", "сто", "двести", "триста",
    "четыреста", "тысяча", "тысячи", "тысяч",
];

const EN_NUM_WORDS: &[&str] = &[
    "zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten", "eleven",
    "twelve", "thirteen", "fourteen", "fifteen", "sixteen", "seventeen", "eighteen", "nineteen",
    "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety", "hundred",
];

fn token_is_number_like(word: &str, lang: &str) -> bool {
    let w = word.trim_matches(|c: char| !(c.is_alphanumeric() || c == '-'));
    if w.is_empty() {
        return false;
    }
    if w.chars().all(|c| c.is_ascii_digit()) {
        return true;
    }
    let w = w.to_lowercase();
    match lang {
        "ru" => RU_NUM_WORDS.iter().any(|&x| x == w.as_str()),
        "en" => EN_NUM_WORDS.iter().any(|&x| x == w.as_str()),
        _ => {
            RU_NUM_WORDS.iter().any(|&x| x == w.as_str())
                || EN_NUM_WORDS.iter().any(|&x| x == w.as_str())
        }
    }
}

fn looks_like_mental_math(phrase: &str, lang: &str) -> bool {
    let has_digit = phrase.chars().any(|c| c.is_ascii_digit());
    let has_ascii_expr = has_digit
        && (phrase.contains('+')
            || phrase.contains('*')
            || phrase.contains('/')
            || phrase.contains('×')
            || phrase.contains('÷'));

    let has_op_ru = phrase.contains("плюс")
        || phrase.contains("минус")
        || phrase.contains("умнож")
        || (phrase.contains("раздели") && phrase.contains(" на"))
        || (phrase.contains("подели") && phrase.contains(" на"));

    let has_op_en = phrase.contains("plus")
        || phrase.contains("minus")
        || phrase.contains("times")
        || phrase.contains("multiplied")
        || phrase.contains("divide")
        || phrase.contains("divided")
        || phrase.contains(" over ");

    let has_op = match lang {
        "ru" => has_op_ru,
        "en" => has_op_en,
        _ => has_op_ru || has_op_en,
    };

    if !has_op && !has_ascii_expr {
        return false;
    }

    if has_ascii_expr {
        return true;
    }

    let words: Vec<&str> = phrase.split_whitespace().collect();
    let mut count = 0u32;
    for w in &words {
        if token_is_number_like(w, lang) {
            count += 1;
        }
    }

    // Нужно минимум два числа (устный пример), иначе ложные срабатывания вроде «температура плюс 5».
    count >= 2
}

fn try_fetch_mental_math<'a>(
    phrase: &str,
    lang: &str,
    commands: &'a [JCommandsList],
) -> Option<(&'a PathBuf, &'a JCommand)> {
    if !looks_like_mental_math(phrase, lang) {
        return None;
    }
    for cmd_list in commands {
        for cmd in &cmd_list.commands {
            if cmd.id == "mental_math" {
                return Some((&cmd_list.path, cmd));
            }
        }
    }
    warn!("mental_math heuristic matched phrase {:?} but command id mental_math is missing", phrase);
    None
}

pub fn fetch_command<'a>(
    phrase: &str,
    commands: &'a [JCommandsList],
) -> Option<(&'a PathBuf, &'a JCommand)> {
    let lang = i18n::get_language();

    let phrase = phrase.trim().to_lowercase();
    if phrase.is_empty() {
        return None;
    }

    if let Some(m) = try_fetch_mental_math(&phrase, lang.as_str(), commands) {
        info!(
            "Mental math heuristic: '{}' -> cmd '{}'",
            phrase,
            m.1.id
        );
        return Some(m);
    }

    let phrase_chars: Vec<char> = phrase.chars().collect();
    let phrase_words: Vec<&str> = phrase.split_whitespace().collect();

    let mut result: Option<(&PathBuf, &JCommand)> = None;
    let mut best_score = config::CMD_RATIO_THRESHOLD;

    for cmd_list in commands {
        for cmd in &cmd_list.commands {
            let cmd_phrases = cmd.get_phrases(&lang);
            
            for cmd_phrase in cmd_phrases.iter() {
                let cmd_phrase_lower = cmd_phrase.trim().to_lowercase();
                let cmd_phrase_chars: Vec<char> = cmd_phrase_lower.chars().collect();
                
                // character-level similarity
                let char_ratio = ratio(&phrase_chars, &cmd_phrase_chars);
                
                // word-level similarity
                let cmd_words: Vec<&str> = cmd_phrase_lower.split_whitespace().collect();
                let word_score = word_overlap_score(&phrase_words, &cmd_words);
                
                // combined score
                let score = (char_ratio * 0.6) + (word_score * 0.4);
                
                // early exit on perfect match
                if score >= 99.0 {
                    debug!("Perfect match: '{}' -> '{}'", phrase, cmd_phrase_lower);
                    return Some((&cmd_list.path, cmd));
                }
                
                if score > best_score {
                    best_score = score;
                    result = Some((&cmd_list.path, cmd));
                }
            }
        }
    }

    if let Some((_, cmd)) = result {
        info!("Fuzzy match: '{}' -> cmd '{}' (score: {:.1}%)", phrase, cmd.id, best_score);
    } else {
        debug!("No match for '{}' (best: {:.1}%)", phrase, best_score);
    }
    
    result
}


fn word_overlap_score(input_words: &[&str], cmd_words: &[&str]) -> f64 {
    if input_words.is_empty() || cmd_words.is_empty() {
        return 0.0;
    }

    let mut matched = 0.0;
    
    // pre-compute cmd word chars to avoid repeated allocations
    let cmd_word_chars: Vec<Vec<char>> = cmd_words
        .iter()
        .map(|w| w.chars().collect())
        .collect();
    
    for input_word in input_words {
        let input_chars: Vec<char> = input_word.chars().collect();
        
        let best_word_match = cmd_word_chars
            .iter()
            .map(|cw| ratio(&input_chars, cw))
            .fold(0.0_f64, f64::max);
        
        if best_word_match > 70.0 {
            matched += best_word_match / 100.0;
        }
    }

    let max_words = input_words.len().max(cmd_words.len()) as f64;
    (matched / max_words) * 100.0
}




pub fn execute_exe(exe: &str, args: &[String]) -> std::io::Result<Child> {
    Command::new(exe).args(args).spawn()
}

pub fn execute_cli(cmd: &str, args: &[String]) -> std::io::Result<Child> {
    debug!("Spawning: cmd /C {} {:?}", cmd, args);

    if cfg!(target_os = "windows") {
        // Some launch contexts (e.g. GUI/tauri) may not have PATH configured for `cmd`.
        // Prefer ComSpec, then fallback to system cmd.exe.
        let mut candidates: Vec<String> = Vec::new();
        if let Ok(comspec) = std::env::var("ComSpec") {
            let comspec = comspec.trim().trim_matches('"').to_string();
            if !comspec.is_empty() {
                candidates.push(comspec);
            }
        }
        candidates.push("cmd".to_string());
        if let Ok(system_root) = std::env::var("SystemRoot") {
            let root = system_root.trim().trim_matches('"');
            if !root.is_empty() {
                candidates.push(format!(r"{}\System32\cmd.exe", root));
                // Needed when a 32-bit process runs on 64-bit Windows and wants native system32.
                candidates.push(format!(r"{}\Sysnative\cmd.exe", root));
            }
        } else {
            candidates.push(r"C:\Windows\System32\cmd.exe".to_string());
            candidates.push(r"C:\Windows\Sysnative\cmd.exe".to_string());
        }

        let mut last_err: Option<std::io::Error> = None;
        let mut failed_shells: Vec<String> = Vec::new();
        for shell in candidates {
            match Command::new(&shell).arg("/C").arg(cmd).args(args).spawn() {
                Ok(child) => return Ok(child),
                Err(e) => {
                    failed_shells.push(format!("{} ({})", shell, e));
                    last_err = Some(e);
                }
            }
        }

        let details = if failed_shells.is_empty() {
            "no shell candidates".to_string()
        } else {
            failed_shells.join("; ")
        };
        Err(last_err.unwrap_or_else(|| std::io::Error::other(format!(
            "Failed to spawn cmd shell: {}",
            details
        ))))
    } else {
        Command::new("sh").arg("-c").arg(cmd).args(args).spawn()
    }
}

fn resolve_ahk_exec_path(cmd_path: &Path, configured_path: &str) -> Result<PathBuf, String> {
    let configured = Path::new(configured_path);
    let local = cmd_path.join(configured_path);

    if configured.exists() {
        return Ok(configured.to_path_buf());
    }
    if local.exists() {
        return Ok(local);
    }

    // Legacy packs point to *.exe, but resources currently ship *.ahk scripts.
    if configured_path.to_ascii_lowercase().ends_with(".exe") {
        let script_rel = configured_path[..configured_path.len() - 4].to_string() + ".ahk";
        let script_abs = Path::new(&script_rel);
        let script_local = cmd_path.join(&script_rel);

        if script_abs.exists() {
            return Ok(script_abs.to_path_buf());
        }
        if script_local.exists() {
            return Ok(script_local);
        }
    }

    Err(format!(
        "AHK target not found: '{}' (checked absolute/local and .ahk fallback)",
        configured_path
    ))
}

pub fn execute_command(
    cmd_path: &PathBuf,
    cmd_config: &JCommand,
    #[cfg_attr(not(feature = "lua"), allow(unused_variables))]
    phrase: Option<&str>,
    #[cfg_attr(not(feature = "lua"), allow(unused_variables))]
    slots: Option<&HashMap<String, SlotValue>>,
) -> Result<bool, String> {
    // execute command by the type
    match cmd_config.cmd_type.as_str() {

        // BRUH
        "voice" => Ok(true),
        
        // LUA command
        #[cfg(feature = "lua")]
        "lua" => {
            execute_lua_command(cmd_path, cmd_config, phrase, slots)
        }

        // AutoHotkey command
        "ahk" => {
            let ahk_path = resolve_ahk_exec_path(cmd_path.as_path(), &cmd_config.exe_path)?;
            let ahk_path_str = ahk_path.to_string_lossy().to_string();
            let is_ahk_script = ahk_path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.eq_ignore_ascii_case("ahk"))
                .unwrap_or(false);

            if is_ahk_script && cfg!(target_os = "windows") {
                // `.ahk` is a script, not an executable binary.
                // On Windows run it via `cmd /C` so file association can resolve AutoHotkey.
                let mut cmdline = format!("\"{}\"", ahk_path_str);
                for arg in &cmd_config.exe_args {
                    cmdline.push(' ');
                    cmdline.push_str(arg);
                }

                execute_cli(&cmdline, &[])
                    .map(|_| true)
                    .map_err(|e| format!("AHK process spawn error: {}", e))
            } else {
                execute_exe(&ahk_path_str, &cmd_config.exe_args)
                    .map(|_| true)
                    .map_err(|e| format!("AHK process spawn error: {}", e))
            }
        }
        
        // CLI command type
        // @TODO: Consider security restrictions
        "cli" => {
            execute_cli(&cmd_config.cli_cmd, &cmd_config.cli_args)
                .map(|_| true)
                .map_err(|e| format!("CLI command error: {}", e))
        }
        
        // TERMINATOR command (T1000)
        "terminate" => {
            std::thread::sleep(Duration::from_secs(2));
            std::process::exit(0);
        }
        
        // STOP CHANING
        "stop_chaining" => Ok(false),

        // other
        _ => {
            error!("Command type unknown: {}", cmd_config.cmd_type);
            Err(format!("Command type unknown: {}", cmd_config.cmd_type).into())
        }
    }
}

// look up a command by its ID
pub fn get_command_by_id<'a>(
    commands: &'a [JCommandsList],
    id: &str,
) -> Option<(&'a PathBuf, &'a JCommand)> {
    for cmd_list in commands {
        for cmd in &cmd_list.commands {
            if cmd.id == id {
                return Some((&cmd_list.path, cmd));
            }
        }
    }
    None
}

pub fn list_paths(commands: &[JCommandsList]) -> Vec<&Path> {
    commands.iter().map(|x| x.path.as_path()).collect()
}

#[cfg(feature = "lua")]
fn execute_lua_command(
    cmd_path: &PathBuf,
    cmd_config: &JCommand,
    phrase: Option<&str>,
    slots: Option<&HashMap<String, SlotValue>>
) -> Result<bool, String> {
    // get script path

    let script_name = if cmd_config.script.is_empty() {
        "script.lua"
    } else {
        &cmd_config.script
    };
    
    let script_path = cmd_path.join(script_name);
    
    if !script_path.exists() {
        return Err(format!("Lua script not found: {}", script_path.display()));
    }
    
    // parse sandbox level
    let sandbox = SandboxLevel::from_str(&cmd_config.sandbox);

    // create context
    let context = CommandContext {
        phrase: phrase.unwrap_or("").to_string(),
        command_id: cmd_config.id.clone(),
        command_path: cmd_path.clone(),
        language: i18n::get_language(),
        slots: slots.map(|s| s.clone()),
    };
    
    // get timeout
    let timeout = Duration::from_millis(cmd_config.timeout);
    
    info!("Executing Lua command: {} (sandbox: {:?}, timeout: {:?})", 
          cmd_config.id, sandbox, timeout);
    
    // execute
    match lua::execute(&script_path, context, sandbox, timeout) {
        Ok(result) => {
            info!("Lua command {} completed (chain: {})", cmd_config.id, result.chain);
            Ok(result.chain)
        }
        Err(e) => {
            error!("Lua command {} failed: {}", cmd_config.id, e);
            Err(e.to_string())
        }
    }
}