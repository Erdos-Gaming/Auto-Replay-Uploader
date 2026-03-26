use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};
use winreg::RegKey;

const REG_PATH: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const REG_VALUE: &str = "ErdosAutoUploader";

pub fn is_enabled() -> bool {
    let Ok(key) = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(REG_PATH, KEY_READ)
    else {
        return false;
    };
    match key.get_value::<String, _>(REG_VALUE) {
        Ok(val) => val.eq_ignore_ascii_case(&current_exe_path()),
        Err(_) => false,
    }
}

pub fn enable() -> anyhow::Result<()> {
    let (key, _) = RegKey::predef(HKEY_CURRENT_USER)
        .create_subkey_with_flags(REG_PATH, KEY_WRITE)?;
    key.set_value(REG_VALUE, &current_exe_path())?;
    log::info!("Launch at startup enabled.");
    Ok(())
}

pub fn disable() -> anyhow::Result<()> {
    let key = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(REG_PATH, KEY_WRITE)?;
    let _ = key.delete_value(REG_VALUE);
    log::info!("Launch at startup disabled.");
    Ok(())
}

pub fn toggle() -> anyhow::Result<bool> {
    if is_enabled() {
        disable()?;
        Ok(false)
    } else {
        enable()?;
        Ok(true)
    }
}

fn current_exe_path() -> String {
    std::env::current_exe()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}
