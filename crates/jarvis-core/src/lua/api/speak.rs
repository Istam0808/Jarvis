//! Озвучивание текста для `jarvis.speak` (встроенные средства ОС).

use crate::i18n;

/// BCP 47 для выбора голоса SAPI / подсказок для `say` / espeak.
fn culture_for_ui_lang(lang: &str) -> &'static str {
    match lang {
        "ru" => "ru-RU",
        "en" => "en-US",
        "ua" => "uk-UA",
        _ => "en-US",
    }
}

pub fn speak_text(text: &str) {
    let t = text.trim();
    if t.is_empty() {
        return;
    }
    let lang = i18n::get_language();

    #[cfg(windows)]
    {
        if let Err(e) = speak_windows(t, lang.as_str()) {
            log::warn!("[speak] Windows SAPI: {}", e);
        }
        return;
    }

    #[cfg(target_os = "macos")]
    {
        if let Err(e) = speak_macos(t, lang.as_str()) {
            log::warn!("[speak] macOS `say`: {}", e);
        }
        return;
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if speak_linux(t, lang.as_str()).is_err() {
            log::warn!("[speak] Linux TTS failed");
        }
    }
}

#[cfg(windows)]
fn speak_windows(text: &str, ui_lang: &str) -> std::io::Result<()> {
    use std::fs;
    use std::process::Command;

    let culture = culture_for_ui_lang(ui_lang);
    // Только безопасные символы для литерала в PowerShell
    if !culture
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "bad culture",
        ));
    }

    let txt = tempfile::NamedTempFile::new()?;
    fs::write(txt.path(), text)?;

    let path_escaped = txt.path().to_string_lossy().replace('\'', "''");
    let script = format!(
        "Add-Type -AssemblyName System.Speech\n\
         $s = New-Object System.Speech.Synthesis.SpeechSynthesizer\n\
         foreach ($iv in $s.GetInstalledVoices()) {{\n\
           if ($iv.VoiceInfo.Culture.Name -eq '{culture}') {{\n\
             $s.SelectVoice($iv.VoiceInfo.Name)\n\
             break\n\
           }}\n\
         }}\n\
         $s.Speak((Get-Content -LiteralPath '{path_escaped}' -Raw -Encoding UTF8))\n",
    );

    let ps = tempfile::NamedTempFile::new()?;
    fs::write(ps.path(), script)?;

    let status = Command::new("powershell")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
            ps.path().to_str().ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::InvalidInput, "temp .ps1 path")
            })?,
        ])
        .status()?;

    if !status.success() {
        log::warn!("[speak] PowerShell exit code: {:?}", status.code());
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn speak_macos(text: &str, ui_lang: &str) -> std::io::Result<()> {
    use std::fs;
    use std::process::Command;

    let f = tempfile::NamedTempFile::new()?;
    fs::write(f.path(), text)?;

    let mut cmd = Command::new("say");
    cmd.arg("-f").arg(f.path());
    // Типичные русские голоса в macOS (если нет — `say` вернёт ошибку)
    match ui_lang {
        "ru" => {
            cmd.args(["-v", "Milena"]);
        }
        "ua" => {
            cmd.args(["-v", "Lesya"]);
        }
        _ => {}
    }
    let st = cmd.status();
    if st.is_err() || !st.as_ref().is_ok_and(|s| s.success()) {
        Command::new("say")
            .arg("-f")
            .arg(f.path())
            .status()?;
    }
    Ok(())
}

#[cfg(all(unix, not(target_os = "macos")))]
fn speak_linux(text: &str, ui_lang: &str) -> std::io::Result<()> {
    use std::process::Command;

    let try_spd = Command::new("spd-say")
        .args(["-l", ui_lang])
        .arg(text)
        .status();
    if try_spd.is_ok_and(|s| s.success()) {
        return Ok(());
    }

    let v = match ui_lang {
        "ru" => "ru",
        "ua" => "uk",
        "en" => "en",
        _ => "en",
    };
    Command::new("espeak-ng")
        .args(["-v", v])
        .arg(text)
        .status()
        .or_else(|_| Command::new("espeak").args(["-v", v]).arg(text).status())?;
    Ok(())
}
