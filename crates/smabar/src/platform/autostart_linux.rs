//! The plugin's auto-launch 0.5 ignores XDG_CONFIG_HOME, Exec quoting and
//! Hidden. Use the already bundled GLib key-file parser for this XDG entry.

use std::fs;
use std::io::ErrorKind;
use std::path::Path;

use anyhow::{Context, ensure};
use gtk::glib::{KeyFile, KeyFileFlags};

const GROUP: &str = "Desktop Entry";

pub(super) fn registered(path: &Path) -> anyhow::Result<bool> {
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error).context("could not read the login entry"),
    };
    let entry = KeyFile::new();
    entry.load_from_data(&content, KeyFileFlags::NONE)?;
    if (entry.has_key(GROUP, "Hidden")? && entry.boolean(GROUP, "Hidden")?)
        || (entry.has_key(GROUP, "X-GNOME-Autostart-enabled")?
            && !entry.boolean(GROUP, "X-GNOME-Autostart-enabled")?)
    {
        return Ok(false);
    }
    ensure!(
        entry.string(GROUP, "Type")? == "Application",
        "login entry is not an application"
    );
    ensure!(
        !entry.string(GROUP, "Exec")?.is_empty(),
        "login entry has no executable"
    );
    Ok(true)
}

pub(super) fn set(path: &Path, executable: &Path, enabled: bool) -> anyhow::Result<()> {
    if !enabled {
        return match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error).context("could not remove the login entry"),
        };
    }
    ensure!(
        executable.is_absolute(),
        "autostart needs an absolute executable path"
    );
    let executable = executable
        .to_str()
        .context("autostart needs a UTF-8 executable path")?;
    // '=' in an executable path is forbidden by the Desktop Entry specification.
    ensure!(
        !executable.contains('='),
        "move smabar to a path without '=' to enable autostart"
    );
    let quoted = executable
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('`', "\\`")
        .replace('$', "\\$")
        .replace('%', "%%");
    let entry = KeyFile::new();
    entry.set_string(GROUP, "Type", "Application");
    entry.set_string(GROUP, "Name", "smabar");
    // KeyFile adds the second layer of string escaping required by XDG.
    entry.set_string(GROUP, "Exec", &format!("\"{quoted}\""));
    entry.set_boolean(GROUP, "Terminal", false);
    entry.set_boolean(GROUP, "StartupNotify", false);
    let parent = path
        .parent()
        .context("login entry has no parent directory")?;
    fs::create_dir_all(parent)?;
    let temporary = path.with_extension("desktop.tmp");
    fs::write(&temporary, entry.to_data())?;
    fs::rename(&temporary, path).context("could not save the login entry")
}

#[cfg(test)]
mod tests {
    use super::*;
    use smabar_core::config::SmabarPaths;

    #[test]
    fn real_entry_uses_supplied_xdg_directory_and_preserves_quoted_paths() {
        let dir = tempfile::tempdir().expect("temporary directory");
        let entry_path = SmabarPaths::desktop_autostart_file(&dir.path().join("custom-config"));
        let executable = dir.path().join("Install location/über smabar");
        assert!(!registered(&entry_path).expect("absent"));
        set(&entry_path, &executable, true).expect("register");
        assert!(registered(&entry_path).expect("present"));
        let entry = KeyFile::new();
        entry
            .load_from_file(&entry_path, KeyFileFlags::NONE)
            .expect("parse actual file");
        let command = entry.string(GROUP, "Exec").expect("Exec");
        let argv = gtk::glib::shell_parse_argv(command).expect("parse executable");
        assert_eq!(argv, vec![executable.into_os_string()]);
        entry.set_boolean(GROUP, "Hidden", true);
        fs::write(&entry_path, entry.to_data()).expect("external disable");
        assert!(!registered(&entry_path).expect("respect Hidden"));
        set(&entry_path, Path::new("/usr/bin/smabar"), true).expect("explicit re-enable");
        entry.set_boolean(GROUP, "Hidden", false);
        entry.set_boolean(GROUP, "X-GNOME-Autostart-enabled", false);
        fs::write(&entry_path, entry.to_data()).expect("GNOME disable");
        assert!(!registered(&entry_path).expect("respect GNOME opt-out"));
        set(&entry_path, Path::new("/usr/bin/smabar"), false).expect("remove");
        set(&entry_path, Path::new("/usr/bin/smabar"), false).expect("remove again");
        assert!(!registered(&entry_path).expect("disabled"));
    }

    #[test]
    fn reserved_characters_survive_both_layers_and_invalid_files_fail() {
        let dir = tempfile::tempdir().expect("temporary directory");
        let path = dir.path().join("smabar.desktop");
        let executable = Path::new("/opt/a\\b\"c$d`e&f/smabar");
        set(&path, executable, true).expect("write escaped entry");
        let entry = KeyFile::new();
        entry
            .load_from_file(&path, KeyFileFlags::NONE)
            .expect("parse");
        assert_eq!(
            gtk::glib::shell_parse_argv(entry.string(GROUP, "Exec").expect("Exec")).expect("argv"),
            vec![executable.as_os_str()]
        );
        fs::write(&path, "broken entry").expect("write broken entry");
        registered(&path).expect_err("invalid registration must be visible");
        set(&path, Path::new("relative"), true).expect_err("absolute path required");
    }
}
