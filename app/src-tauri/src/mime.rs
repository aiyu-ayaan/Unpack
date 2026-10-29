//! "Open with Unpack": a hidden menu entry that takes package files, set as their default app, so a
//! double-click in the file manager or the browser's download list opens the file in Unpack's Install view.
use crate::util::{command, has, home, xdg};
use std::path::PathBuf;

const ENTRY: &str = "unpack-open.desktop";

/// Package formats Unpack handles that have a registered type. Scripts and archives are left alone: taking
/// those over would change what double-clicking a `.sh` or a `.zip` does everywhere.
pub const TYPES: [&str; 10] = [
    "application/vnd.debian.binary-package",
    "application/x-deb",
    "application/x-rpm",
    "application/x-redhat-package-manager",
    "application/vnd.appimage",
    "application/x-iso9660-appimage",
    "application/vnd.flatpak",
    "application/vnd.flatpak.ref",
    "application/vnd.flatpak.repo",
    "application/vnd.snap",
];

fn entry_path() -> PathBuf {
    xdg("XDG_DATA_HOME", ".local/share").join("applications").join(ENTRY)
}

pub fn enabled() -> bool {
    entry_path().is_file()
}

/// The program to start: the AppImage file itself when running from one, else this binary.
pub fn program() -> Result<PathBuf, String> {
    match crate::updates::appimage() {
        Some(p) => Ok(p),
        None => std::env::current_exe().map_err(|e| e.to_string()),
    }
}

pub fn desktop_entry(exe: &str) -> String {
    format!(
        "[Desktop Entry]\nType=Application\nName=Unpack\nComment=Install this with Unpack\nExec={exe} open %f\nIcon=unpack\nTerminal=false\nNoDisplay=true\nMimeType={};\n",
        TYPES.join(";")
    )
}

fn refresh() {
    if has("update-desktop-database") {
        let _ = command("update-desktop-database").arg(entry_path().parent().expect("has parent")).output();
    }
}

/// Write the entry and make it the default for every type in `TYPES`.
pub fn enable() -> Result<(), String> {
    let p = entry_path();
    std::fs::create_dir_all(p.parent().expect("has parent")).map_err(|e| e.to_string())?;
    std::fs::write(&p, desktop_entry(&crate::runner::exec_quote(&program()?))).map_err(|e| e.to_string())?;
    refresh();
    if !has("xdg-mime") {
        return Err("Added \"Open with Unpack\", but `xdg-mime` is missing, so it is not the default yet. Pick Unpack in your file manager's \"Open with\" menu.".into());
    }
    let mut args = vec!["default", ENTRY];
    args.extend(TYPES);
    let out = command("xdg-mime").args(&args).output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!("xdg-mime failed: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(())
}

/// `mimeapps.list` without our entry; lines left with no app are dropped.
pub fn without_entry(text: &str) -> String {
    let mut out = vec![];
    for l in text.lines() {
        match l.split_once('=') {
            Some((k, v)) if !l.starts_with('[') && v.contains(ENTRY) => {
                let kept: Vec<&str> = v.split(';').filter(|a| !a.is_empty() && *a != ENTRY).collect();
                if !kept.is_empty() {
                    out.push(format!("{k}={};", kept.join(";")));
                }
            }
            _ => out.push(l.to_string()),
        }
    }
    out.push(String::new());
    out.join("\n")
}

pub fn disable() -> Result<(), String> {
    let p = entry_path();
    if p.exists() {
        std::fs::remove_file(&p).map_err(|e| e.to_string())?;
    }
    for list in [xdg("XDG_CONFIG_HOME", ".config").join("mimeapps.list"), home().join(".local/share/applications/mimeapps.list")] {
        if let Ok(t) = std::fs::read_to_string(&list) {
            if t.contains(ENTRY) {
                std::fs::write(&list, without_entry(&t)).map_err(|e| e.to_string())?;
            }
        }
    }
    refresh();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_and_cleanup() {
        let e = desktop_entry("\"/opt/Unpack.AppImage\"");
        assert!(e.contains("Exec=\"/opt/Unpack.AppImage\" open %f\n") && e.contains("NoDisplay=true") && e.contains("application/x-rpm;"));
        let t = "[Default Applications]\napplication/x-rpm=unpack-open.desktop;\napplication/x-deb=unpack-open.desktop;gdebi.desktop;\ntext/plain=gedit.desktop\n";
        assert_eq!(without_entry(t), "[Default Applications]\napplication/x-deb=gdebi.desktop;\ntext/plain=gedit.desktop\n");
    }
}
