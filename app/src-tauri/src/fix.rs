//! "Fix my install": find what keeps a tracked app from starting and plan the repair.
//! Checks: the runtime its format needs (Flatpak, snapd, FUSE 2), the exec bit on its programs, and shared
//! libraries a native package needs but the system lacks.
use crate::libcheck;
use crate::plan::{bootstrap, fuse2_package, fuse2_present, Plan, Track, Uninstall};
use crate::store::App;
use crate::system::System;
use crate::util::has;
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

fn is_appimage(p: &Path) -> bool {
    p.extension().is_some_and(|e| e.eq_ignore_ascii_case("appimage"))
}

/// A program (ELF or `#!` script) that lost its exec bit.
fn lost_exec_bit(p: &Path) -> bool {
    let Ok(m) = std::fs::metadata(p) else { return false };
    let mut head = [0u8; 4];
    let program = std::fs::File::open(p).and_then(|mut f| f.read(&mut head)).is_ok_and(|n| n >= 2 && (head.starts_with(b"\x7fELF") || head.starts_with(b"#!")));
    m.is_file() && m.permissions().mode() & 0o111 == 0 && program
}

pub fn plan(app: &App, sys: &System) -> Plan {
    let mut p = Plan::new("fix", &app.name, &format!("Unpack record {}", app.id));
    p.title = format!("Fix {}", app.name);
    p.track = Track::Nothing;
    match &app.uninstall {
        Uninstall::Manager { manager, .. } if manager == "flatpak" && !has("flatpak") => bootstrap(&mut p, sys, "Install Flatpak", &["flatpak"]),
        Uninstall::Manager { manager, .. } if manager == "snap" && !has("snap") => bootstrap(&mut p, sys, "Install Snap support", &["snapd"]),
        Uninstall::Manager { manager, ident, user: false } => {
            let miss = libcheck::missing(manager, ident);
            let mut pkgs: Vec<String> = miss.iter().filter_map(|m| libcheck::provider(manager, m)).collect();
            pkgs.sort();
            pkgs.dedup();
            if !pkgs.is_empty() {
                let refs: Vec<&str> = pkgs.iter().map(String::as_str).collect();
                bootstrap(&mut p, sys, "Install missing libraries", &refs);
            }
            if !miss.is_empty() && pkgs.len() < miss.len() {
                p.warnings.push(format!("Some missing libraries have no known package here: {}. The app's website may list what it needs.", miss.join(", ")));
            }
        }
        Uninstall::Files { paths } => {
            let paths: Vec<&Path> = paths.iter().map(Path::new).filter(|p| p.exists()).collect();
            for f in paths.iter().filter(|f| lost_exec_bit(f)) {
                p.cmd("Make it runnable again", false, "chmod", &["+x", "--", &f.to_string_lossy()], None);
            }
            if paths.iter().any(|f| is_appimage(f)) && !fuse2_present() {
                match sys.native {
                    Some(n) => bootstrap(&mut p, sys, "Install FUSE 2 (needed by AppImage)", &[fuse2_package(n)]),
                    None => p.warnings.push("AppImages need FUSE 2 (libfuse.so.2), which is missing. Install it with your system's tools.".into()),
                }
            }
            if paths.is_empty() {
                p.warnings.push("None of this app's files are left. Install it again.".into());
            }
        }
        _ => {}
    }
    if p.steps.is_empty() {
        p.supported = false;
        if p.warnings.is_empty() {
            p.advice.push("No problems found: its runtime is present, its programs can run and no libraries are missing.".into());
        }
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restores_a_lost_exec_bit() {
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join("tool");
        std::fs::write(&f, "#!/bin/sh\necho hi\n").unwrap();
        std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o644)).unwrap();
        let app = App { id: 1, name: "tool".into(), version: None, kind: "x".into(), source: None, sha256: None, installed_at: 0, status: "installed".into(), replaced_by: None, uninstall: Uninstall::Files { paths: vec![f.to_string_lossy().into()] } };
        let p = plan(&app, &crate::system::detect());
        assert!(p.steps.iter().any(|s| s.display.starts_with("chmod +x --")), "{:?}", p.steps);
        std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o755)).unwrap();
        let ok = plan(&app, &crate::system::detect());
        assert!(!ok.supported && !ok.advice.is_empty());
    }
}
