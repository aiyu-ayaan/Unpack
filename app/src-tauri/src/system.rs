use crate::util::{has, which};
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Family {
    Debian,
    RedHat,
    Suse,
    Arch,
    Alpine,
    Void,
    Gentoo,
    Nix,
    Unknown,
}

/// Native package manager we know how to drive for local-file installs.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Native {
    Apt,
    Dnf,
    Zypper,
    Pacman,
    Apk,
}

#[derive(Serialize, Clone, Debug)]
pub struct System {
    pub distro: String,
    pub id: String,
    pub version: String,
    pub family: Family,
    pub native: Option<Native>,
    pub arch: String,
    pub managers: Vec<String>,
    pub runtimes: Vec<String>,
    pub helpers: Vec<String>,
    pub missing_helpers: Vec<String>,
    pub immutable: bool,
    /// On a read-only base: the tool that adds packages to it (`rpm-ostree`, `transactional-update`), if any.
    pub layer: Option<String>,
    /// The standard glibc loader is missing (NixOS without nix-ld): binaries built for other distros can't start.
    pub no_fhs_loader: bool,
    pub musl: bool,
    pub is_root: bool,
}

const MANAGERS: [&str; 11] = [
    "apt-get", "dnf", "yum", "zypper", "pacman", "apk", "xbps-install", "emerge", "nix",
    "rpm-ostree", "transactional-update",
];
const RUNTIMES: [&str; 4] = ["flatpak", "snap", "distrobox", "toolbox"];
const HELPERS: [&str; 14] = [
    "pkexec", "sudo", "git", "curl", "wget", "tar", "unzip", "7z", "bsdtar", "unsquashfs",
    "dpkg-deb", "rpm", "desktop-file-validate", "update-desktop-database",
];

pub fn parse_os_release(text: &str) -> HashMap<String, String> {
    text.lines()
        .filter_map(|l| {
            let (k, v) = l.split_once('=')?;
            Some((k.trim().to_string(), v.trim().trim_matches(['"', '\'']).to_string()))
        })
        .collect()
}

pub fn family_of(id: &str, id_like: &str) -> Family {
    let ids: Vec<&str> = std::iter::once(id).chain(id_like.split_whitespace()).collect();
    for i in ids {
        match i {
            "debian" | "ubuntu" => return Family::Debian,
            "fedora" | "rhel" | "centos" => return Family::RedHat,
            "suse" | "opensuse" | "opensuse-leap" | "opensuse-tumbleweed" | "sles" => {
                return Family::Suse
            }
            "arch" => return Family::Arch,
            "alpine" => return Family::Alpine,
            "void" => return Family::Void,
            "gentoo" => return Family::Gentoo,
            "nixos" => return Family::Nix,
            _ => {}
        }
    }
    Family::Unknown
}

fn native_for(family: Family) -> Option<Native> {
    let by_family = match family {
        Family::Debian => Some(Native::Apt),
        Family::RedHat => Some(Native::Dnf),
        Family::Suse => Some(Native::Zypper),
        Family::Arch => Some(Native::Pacman),
        Family::Alpine => Some(Native::Apk),
        _ => None,
    };
    let present = |n: Native| match n {
        Native::Apt => has("apt-get"),
        Native::Dnf => has("dnf") || has("yum"),
        Native::Zypper => has("zypper"),
        Native::Pacman => has("pacman"),
        Native::Apk => has("apk"),
    };
    // Trust the family only when its tool exists; otherwise fall back to whatever is on PATH.
    by_family.filter(|n| present(*n)).or_else(|| {
        [Native::Apt, Native::Dnf, Native::Zypper, Native::Pacman, Native::Apk]
            .into_iter()
            .find(|n| present(*n))
    })
}

fn effective_uid() -> Option<u32> {
    let s = std::fs::read_to_string("/proc/self/status").ok()?;
    s.lines().find(|l| l.starts_with("Uid:"))?.split_whitespace().nth(2)?.parse().ok()
}

fn is_musl() -> bool {
    std::fs::read_dir("/lib")
        .map(|d| d.flatten().any(|e| e.file_name().to_string_lossy().starts_with("ld-musl-")))
        .unwrap_or(false)
}

/// The glibc dynamic loader programs from other distros ask for, for this CPU.
pub fn glibc_loader() -> Option<&'static str> {
    match std::env::consts::ARCH {
        "x86_64" => Some("/lib64/ld-linux-x86-64.so.2"),
        "aarch64" => Some("/lib/ld-linux-aarch64.so.1"),
        _ => None,
    }
}

fn layer_tool(ostree: bool) -> Option<String> {
    if ostree && has("rpm-ostree") {
        Some("rpm-ostree".into())
    } else if has("transactional-update") {
        Some("transactional-update".into())
    } else {
        None
    }
}

pub fn detect() -> System {
    let text = std::fs::read_to_string("/etc/os-release")
        .or_else(|_| std::fs::read_to_string("/usr/lib/os-release"))
        .unwrap_or_default();
    let os = parse_os_release(&text);
    let get = |k: &str| os.get(k).cloned().unwrap_or_default();
    let family = family_of(&get("ID"), &get("ID_LIKE"));
    let found = |list: &[&str]| -> Vec<String> {
        list.iter().filter(|b| which(b).is_some()).map(|b| b.to_string()).collect()
    };
    let helpers = found(&HELPERS);
    let musl = is_musl();
    let missing_helpers = HELPERS
        .iter()
        .filter(|h| !helpers.iter().any(|f| f == *h))
        .map(|h| h.to_string())
        .collect();
    System {
        distro: os.get("PRETTY_NAME").cloned().unwrap_or_else(|| "Linux".into()),
        id: get("ID"),
        version: get("VERSION_ID"),
        family,
        native: native_for(family),
        arch: std::env::consts::ARCH.to_string(),
        managers: found(&MANAGERS),
        runtimes: found(&RUNTIMES),
        helpers,
        missing_helpers,
        // SteamOS ships `steamos-readonly` to toggle its read-only root.
        immutable: Path::new("/run/ostree-booted").exists() || has("transactional-update") || has("steamos-readonly"),
        layer: layer_tool(Path::new("/run/ostree-booted").exists()),
        no_fhs_loader: !musl && glibc_loader().is_some_and(|l| !Path::new(l).exists()),
        musl,
        is_root: effective_uid() == Some(0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_os_release() {
        let m = parse_os_release("ID=ubuntu\nID_LIKE=\"debian\"\nPRETTY_NAME=\"Ubuntu 24.04 LTS\"\n# c\n");
        assert_eq!(m["ID"], "ubuntu");
        assert_eq!(m["ID_LIKE"], "debian");
        assert_eq!(m["PRETTY_NAME"], "Ubuntu 24.04 LTS");
    }

    #[test]
    fn families() {
        assert_eq!(family_of("ubuntu", "debian"), Family::Debian);
        assert_eq!(family_of("linuxmint", "ubuntu debian"), Family::Debian);
        assert_eq!(family_of("nobara", "rhel centos fedora"), Family::RedHat);
        assert_eq!(family_of("opensuse-tumbleweed", "opensuse suse"), Family::Suse);
        assert_eq!(family_of("manjaro", "arch"), Family::Arch);
        assert_eq!(family_of("endeavouros", "arch"), Family::Arch);
        assert_eq!(family_of("alpine", ""), Family::Alpine);
        assert_eq!(family_of("nixos", ""), Family::Nix);
        assert_eq!(family_of("plan9", ""), Family::Unknown);
    }
}
