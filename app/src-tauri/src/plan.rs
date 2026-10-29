//! Turn a classified artifact plus the host system into a reviewable install plan.
use crate::archive::{Build, Content};
use crate::sniff::{Archive, Kind};
use crate::system::{Family, Native, System};
use crate::util::{capture, display, has, home};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub enum Action {
    Cmd { program: String, args: Vec<String>, cwd: Option<PathBuf> },
    Extract { archive: PathBuf, fmt: Archive, dest: PathBuf },
    /// `no_sandbox`: the menu entry starts the app with `--no-sandbox` (Electron on a system that blocks its sandbox).
    IntegrateAppImage { src: PathBuf, name: String, no_sandbox: bool },
    InstallPortable { from: PathBuf, name: String },
    InstallBinary { src: PathBuf, name: String },
    /// Find a package produced by an earlier build step and install it natively.
    InstallBuilt { dirs: Vec<PathBuf>, ext: String },
}

#[derive(Serialize, Clone, Debug)]
pub struct Step {
    pub label: String,
    pub display: String,
    pub needs_root: bool,
    #[serde(skip)]
    pub action: Action,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Uninstall {
    Manager { manager: String, ident: String, user: bool },
    Files { paths: Vec<String> },
    /// Files are filled in after the run from a before/after filesystem diff.
    Traced,
    Manual { note: String },
}

/// What a successful run does to the database.
#[derive(Clone, Debug, PartialEq)]
pub enum Track {
    /// Record a new app.
    New,
    /// Record a new app and remove the files of this older record (an update that is a new download).
    Replace(i64),
    /// Record nothing: a fix or an in-place update of something already there.
    Nothing,
}

#[derive(Serialize, Clone, Debug)]
pub struct Plan {
    pub id: String,
    pub title: String,
    pub kind: String,
    pub name: String,
    pub version: Option<String>,
    pub source: String,
    /// Where a download really came from after redirects, when that differs from `source`.
    pub fetched: Option<String>,
    pub sha256: Option<String>,
    pub warnings: Vec<String>,
    pub advice: Vec<String>,
    pub steps: Vec<Step>,
    pub supported: bool,
    /// Script text or archive member list worth showing before running.
    pub preview: Option<String>,
    /// User may choose to run the main step as administrator.
    pub root_optional: bool,
    pub needs_root: bool,
    #[serde(skip)]
    pub uninstall: Uninstall,
    #[serde(skip)]
    pub trace: bool,
    #[serde(skip)]
    pub discover_flatpak: bool,
    #[serde(skip)]
    pub verify: Option<(PathBuf, String)>,
    #[serde(skip)]
    pub track: Track,
}

impl Plan {
    pub fn new(kind: &str, name: &str, source: &str) -> Plan {
        Plan {
            id: String::new(),
            title: format!("Install {name}"),
            kind: kind.into(),
            name: name.into(),
            version: None,
            source: source.into(),
            fetched: None,
            sha256: None,
            warnings: vec![],
            advice: vec![],
            steps: vec![],
            supported: true,
            preview: None,
            root_optional: false,
            needs_root: false,
            uninstall: Uninstall::Traced,
            trace: false,
            discover_flatpak: false,
            verify: None,
            track: Track::New,
        }
    }

    pub fn step(&mut self, label: &str, root: bool, action: Action) {
        let shown = match &action {
            Action::Cmd { program, args, .. } => display(program, args),
            Action::Extract { archive, dest, .. } => format!("extract {} to {}", archive.display(), dest.display()),
            Action::IntegrateAppImage { name, .. } => format!("add {name} to the app menu"),
            Action::InstallPortable { name, .. } => format!("install {name} into ~/.local/opt"),
            Action::InstallBinary { name, .. } => format!("copy {name} to ~/.local/bin"),
            Action::InstallBuilt { ext, .. } => format!("install the built .{ext} package"),
        };
        self.needs_root |= root;
        self.steps.push(Step { label: label.into(), display: shown, needs_root: root, action });
    }

    pub fn cmd(&mut self, label: &str, root: bool, program: &str, args: &[&str], cwd: Option<&Path>) {
        let args = args.iter().map(|s| s.to_string()).collect();
        self.step(label, root, Action::Cmd { program: program.into(), args, cwd: cwd.map(Path::to_path_buf) });
    }

    pub fn refuse(&mut self, why: &str, advice: &[&str]) {
        self.supported = false;
        self.warnings.push(why.into());
        self.advice.extend(advice.iter().map(|s| s.to_string()));
    }
}

/// What the orchestrator found; `path` is a file, or the source directory for `Source`.
pub enum Input {
    Package { path: PathBuf, kind: Kind },
    Archive { path: PathBuf, fmt: Archive, content: Content, top: String, members: Vec<String>, work: PathBuf },
    Repo { dir: PathBuf, content: Content },
    Dsc { path: PathBuf, work: PathBuf },
}

pub struct Meta {
    pub source: String,
    pub sha256: Option<String>,
    pub pipe_to_shell: bool,
}

pub fn build(input: &Input, sys: &System, meta: &Meta) -> Plan {
    let mut plan = match input {
        Input::Package { path, kind } => plan_package(path, *kind, sys),
        Input::Archive { path, fmt, content, top, members, work } => {
            plan_archive(path, *fmt, content, top, members, work, sys)
        }
        Input::Repo { dir, content } => plan_content(content, dir, &file_stem(dir), sys),
        Input::Dsc { path, work } => plan_dsc(path, work, sys),
    };
    plan.source = meta.source.clone();
    plan.sha256 = meta.sha256.clone();
    if meta.pipe_to_shell {
        plan.warnings.insert(
            0,
            "This came from a 'download and run' command. Unpack saved the script and will run that saved copy only after you approve.".into(),
        );
    }
    if plan.supported {
        if sys.immutable {
            plan.warnings.push("This system has a read-only base. System-wide package installs may need a reboot or may be undone by updates; Flatpak or AppImage is safer.".into());
        }
        if plan.needs_root && !sys.is_root && !has("pkexec") {
            plan.warnings.push("Admin rights are needed but `pkexec` is missing. Install `polkit` or run the shown commands in a terminal.".into());
        }
    }
    plan.title = format!("Install {}", plan.name);
    plan
}

// ---------------------------------------------------------------- helpers

pub fn file_stem(p: &Path) -> String {
    let n = p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let mut n = n.as_str();
    for ext in [".pkg.tar.zst", ".pkg.tar.xz", ".tar.gz", ".tar.xz", ".tar.bz2", ".tar.zst", ".AppImage", ".appimage", ".tgz", ".txz", ".tbz2", ".zip", ".7z", ".deb", ".rpm", ".snap", ".flatpak", ".flatpakref", ".flatpakrepo", ".apk", ".run", ".bin", ".sh", ".dsc", ".tar", ".gz", ".xz", ".bz2", ".zst"] {
        if let Some(s) = n.strip_suffix(ext) {
            n = s;
            break;
        }
    }
    n.to_string()
}

pub fn install_pkgs(native: Native, pkgs: &[&str]) -> (String, Vec<String>) {
    let mut a: Vec<String> = vec![];
    let prog = match native {
        Native::Apt => {
            a.extend(["DEBIAN_FRONTEND=noninteractive", "apt-get", "install", "-y"].map(String::from));
            "env"
        }
        Native::Dnf => {
            a.extend(["install", "-y"].map(String::from));
            if has("dnf") { "dnf" } else { "yum" }
        }
        Native::Zypper => {
            a.extend(["--non-interactive", "install"].map(String::from));
            "zypper"
        }
        Native::Pacman => {
            a.extend(["-S", "--noconfirm", "--needed"].map(String::from));
            "pacman"
        }
        Native::Apk => {
            a.push("add".into());
            "apk"
        }
    };
    a.extend(pkgs.iter().map(|s| s.to_string()));
    (prog.into(), a)
}

pub fn bootstrap(plan: &mut Plan, sys: &System, label: &str, pkgs: &[&str]) {
    match sys.native {
        Some(n) => {
            let (p, a) = install_pkgs(n, pkgs);
            plan.step(label, true, Action::Cmd { program: p, args: a, cwd: None });
        }
        None => plan.refuse("No supported package manager found to install the missing tools.", &[]),
    }
}

fn host_deb_arch() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        "arm" => "armhf",
        "riscv64" => "riscv64",
        other => other,
    }
}

fn arch_ok(pkg_arch: &str) -> bool {
    let host = std::env::consts::ARCH;
    matches!(pkg_arch, "all" | "noarch" | "any") || pkg_arch == host || pkg_arch == host_deb_arch()
}

fn field(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|l| l.strip_prefix(key).map(|v| v.trim().to_string()))
}

fn pkginfo(path: &Path) -> Option<String> {
    let p = path.to_string_lossy();
    for prog in ["bsdtar", "tar"] {
        if has(prog) {
            if let Ok(o) = capture(prog, &["-xOf", &p, ".PKGINFO"]) {
                return Some(o);
            }
        }
    }
    None
}

fn unsupported_foreign(plan: &mut Plan, what: &str, sys: &System, native_fmt: &str) {
    plan.refuse(
        &format!("{what} can't be installed natively on {} (it uses {native_fmt}).", sys.distro),
        &[
            "Look for a Flatpak, AppImage or a package made for your distro on the app's website.",
            "Search your software center for the same app name.",
        ],
    );
}

// ---------------------------------------------------------------- packages

fn plan_package(path: &Path, kind: Kind, sys: &System) -> Plan {
    let fname = file_stem(path);
    let abs = path.to_string_lossy().to_string();
    let mut p = Plan::new(&format!("{kind:?}"), &fname, "");
    match kind {
        Kind::Deb => {
            let info = capture("dpkg-deb", &["-f", &abs, "Package", "Version", "Architecture"]);
            if sys.native != Some(Native::Apt) {
                p.name = fname;
                unsupported_foreign(&mut p, "A .deb package", sys, "a different package format");
                return p;
            }
            // Without the package name the uninstall record would be wrong, so never guess it from the file name.
            let info = match info {
                Ok(i) if field(&i, "Package:").is_some() => i,
                other => {
                    p.name = fname;
                    let why = other.err().unwrap_or_else(|| "it has no Package field".into());
                    p.refuse(&format!("Could not read this package: {why}"), &["Download the file again; it may be incomplete or not a .deb."]);
                    return p;
                }
            };
            let name = field(&info, "Package:").unwrap_or_default();
            p.name = name.clone();
            p.version = field(&info, "Version:");
            if let Some(a) = field(&info, "Architecture:") {
                if !arch_ok(&a) {
                    p.refuse(&format!("This package is built for {a}, but this computer is {}.", host_deb_arch()), &["Download the version for your CPU."]);
                    return p;
                }
            }
            p.step("Install package with dependencies", true, Action::Cmd { program: "env".into(), args: ["DEBIAN_FRONTEND=noninteractive", "apt-get", "install", "-y", &abs].map(String::from).to_vec(), cwd: None });
            p.uninstall = Uninstall::Manager { manager: "apt".into(), ident: name, user: false };
        }
        Kind::Rpm => {
            let info = capture("rpm", &["-qp", "--queryformat", "%{NAME}\n%{VERSION}-%{RELEASE}\n%{ARCH}\n", &abs]).unwrap_or_default();
            let mut l = info.lines();
            let name = l.next().filter(|s| !s.is_empty()).map(String::from).unwrap_or(fname);
            p.name = name.clone();
            p.version = l.next().map(String::from);
            let (manager, prog, args): (&str, &str, Vec<&str>) = match sys.native {
                Some(Native::Dnf) => ("dnf", if has("dnf") { "dnf" } else { "yum" }, vec!["install", "-y", &abs]),
                Some(Native::Zypper) => {
                    p.warnings.push("openSUSE will install this even if it is unsigned (--allow-unsigned-rpm). Only continue if you trust the source.".into());
                    ("zypper", "zypper", vec!["--non-interactive", "install", "--allow-unsigned-rpm", &abs])
                }
                _ => {
                    unsupported_foreign(&mut p, "An .rpm package", sys, "a different package format");
                    return p;
                }
            };
            if let Some(a) = l.next() {
                if !arch_ok(a) {
                    p.refuse(&format!("This package is built for {a}, but this computer is {}.", std::env::consts::ARCH), &["Download the version for your CPU."]);
                    return p;
                }
            }
            p.cmd("Install package with dependencies", true, prog, &args, None);
            p.uninstall = Uninstall::Manager { manager: manager.into(), ident: name, user: false };
        }
        Kind::PacmanPkg => {
            let info = pkginfo(path).unwrap_or_default();
            p.name = field(&info, "pkgname =").unwrap_or(fname);
            p.version = field(&info, "pkgver =");
            if sys.native != Some(Native::Pacman) {
                unsupported_foreign(&mut p, "An Arch package", sys, "a different package format");
                return p;
            }
            if let Some(a) = field(&info, "arch =") {
                if !arch_ok(&a) {
                    p.refuse(&format!("This package is built for {a}, but this computer is {}.", std::env::consts::ARCH), &[]);
                    return p;
                }
            }
            p.cmd("Install package", true, "pacman", &["-U", "--noconfirm", &abs], None);
            p.uninstall = Uninstall::Manager { manager: "pacman".into(), ident: p.name.clone(), user: false };
        }
        Kind::AlpineApk => {
            let info = pkginfo(path).unwrap_or_default();
            p.name = field(&info, "pkgname =").unwrap_or(fname);
            p.version = field(&info, "pkgver =");
            if sys.native != Some(Native::Apk) {
                unsupported_foreign(&mut p, "An Alpine package", sys, "a different package format");
                return p;
            }
            p.warnings.push("Local Alpine packages are installed with --allow-untrusted (no signature check). Only continue if you trust the source.".into());
            p.cmd("Install package", true, "apk", &["add", "--allow-untrusted", &abs], None);
            p.uninstall = Uninstall::Manager { manager: "apk".into(), ident: p.name.clone(), user: false };
        }
        Kind::AndroidApk => p.refuse(
            "This is an Android app (.apk), not a Linux program.",
            &["Android apps don't run on Linux directly. Look for the Linux version, or use Waydroid."],
        ),
        Kind::Snap => plan_snap(&mut p, path, &abs, sys),
        Kind::FlatpakBundle | Kind::FlatpakRef | Kind::FlatpakRepo => plan_flatpak(&mut p, kind, &abs, sys),
        Kind::AppImage => plan_appimage(&mut p, path, sys),
        Kind::Script | Kind::RunInstaller | Kind::BinInstaller => plan_script(&mut p, path, kind),
        Kind::Elf => {
            p.title = format!("Install {fname}");
            p.name = fname.clone();
            p.step("Copy program into ~/.local/bin", false, Action::InstallBinary { src: path.to_path_buf(), name: fname });
            p.uninstall = Uninstall::Traced;
            p.trace = false;
        }
        Kind::Dsc | Kind::Archive(_) | Kind::Unknown => p.refuse(
            "Unpack doesn't recognise this file.",
            &["Check that the download finished and that it is a package, archive or script."],
        ),
    }
    p
}

fn plan_snap(p: &mut Plan, path: &Path, abs: &str, sys: &System) {
    if has("unsquashfs") {
        if let Ok(y) = capture("unsquashfs", &["-cat", abs, "meta/snap.yaml"]) {
            if let Some(n) = field(&y, "name:") { p.name = n; }
            p.version = field(&y, "version:").map(|v| v.trim_matches(['"', '\'']).to_string());
            if field(&y, "confinement:").as_deref() == Some("classic") {
                p.warnings.push("This snap uses classic confinement (full system access).".into());
                return finish_snap(p, abs, sys, true);
            }
        }
    } else if p.name.is_empty() {
        p.name = file_stem(path);
    }
    finish_snap(p, abs, sys, false)
}

fn finish_snap(p: &mut Plan, abs: &str, sys: &System, classic: bool) {
    if !has("snap") {
        if sys.native == Some(Native::Apt) {
            bootstrap(p, sys, "Install Snap support", &["snapd"]);
        } else {
            p.refuse("Snap support (snapd) isn't installed and can't be set up automatically on this system.", &["Look for a Flatpak or AppImage of the same app."]);
            return;
        }
    }
    p.warnings.push("Local snaps are installed with --dangerous (not verified by the Snap Store).".into());
    let mut args = vec!["install", "--dangerous"];
    if classic { args.push("--classic"); }
    args.push(abs);
    p.cmd("Install snap", true, "snap", &args, None);
    p.uninstall = Uninstall::Manager { manager: "snap".into(), ident: p.name.clone(), user: false };
}

fn plan_flatpak(p: &mut Plan, kind: Kind, abs: &str, sys: &System) {
    if !has("flatpak") {
        bootstrap(p, sys, "Install Flatpak", &["flatpak"]);
        if !p.supported { return; }
    }
    let text = if kind == Kind::FlatpakBundle { String::new() } else { std::fs::read_to_string(abs).unwrap_or_default() };
    if let Some(n) = field(&text, "Name=") { p.name = n; }
    p.version = field(&text, "Branch=");
    match kind {
        Kind::FlatpakRepo => {
            let name = field(&text, "Title=").map(|_| file_stem(Path::new(abs))).unwrap_or_else(|| file_stem(Path::new(abs)));
            p.name = name.clone();
            p.title = format!("Add Flatpak source {name}");
            p.cmd("Add source", false, "flatpak", &["remote-add", "--user", "--if-not-exists", &name, abs], None);
            p.uninstall = Uninstall::Manager { manager: "flatpak-remote".into(), ident: name, user: true };
        }
        _ => {
            p.cmd("Add Flathub (for runtimes)", false, "flatpak", &["remote-add", "--user", "--if-not-exists", "flathub", "https://dl.flathub.org/repo/flathub.flatpakrepo"], None);
            let flag = if kind == Kind::FlatpakBundle { "--bundle" } else { "--from" };
            p.cmd("Install Flatpak app", false, "flatpak", &["install", "--user", "-y", flag, abs], None);
            p.discover_flatpak = true;
            p.uninstall = Uninstall::Manager { manager: "flatpak".into(), ident: p.name.clone(), user: true };
        }
    }
}

pub fn fuse2_present() -> bool {
    if let Ok(o) = capture("ldconfig", &["-p"]) {
        return o.contains("libfuse.so.2");
    }
    ["/usr/lib/libfuse.so.2", "/usr/lib64/libfuse.so.2", "/lib/x86_64-linux-gnu/libfuse.so.2", "/usr/lib/x86_64-linux-gnu/libfuse.so.2", "/usr/lib/aarch64-linux-gnu/libfuse.so.2"]
        .iter()
        .any(|p| Path::new(p).exists())
}

pub fn fuse2_package(native: Native) -> &'static str {
    match native {
        Native::Apt => if capture("apt-cache", &["show", "libfuse2t64"]).is_ok() { "libfuse2t64" } else { "libfuse2" },
        Native::Dnf => "fuse-libs",
        Native::Zypper => "libfuse2",
        Native::Pacman => "fuse2",
        Native::Apk => "fuse",
    }
}

/// Byte offset of the squashfs inside a type 2 AppImage: the end of the ELF section header table.
fn appimage_offset(path: &Path) -> Option<u64> {
    use std::io::Read;
    let mut h = [0u8; 64];
    std::fs::File::open(path).ok()?.read_exact(&mut h).ok()?;
    if !h.starts_with(b"\x7fELF") || h[4] != 2 {
        return None;
    }
    let shoff = u64::from_le_bytes(h[0x28..0x30].try_into().ok()?);
    let shentsize = u16::from_le_bytes(h[0x3A..0x3C].try_into().ok()?) as u64;
    let shnum = u16::from_le_bytes(h[0x3C..0x3E].try_into().ok()?) as u64;
    Some(shoff + shentsize * shnum)
}

/// Electron and Chromium apps carry a `chrome-sandbox` helper. Listing the image does not run it.
fn is_electron(path: &Path) -> bool {
    let Some(off) = appimage_offset(path) else { return false };
    has("unsquashfs") && capture("unsquashfs", &["-o", &off.to_string(), "-l", &path.to_string_lossy()]).is_ok_and(|l| l.lines().any(|l| l.ends_with("/chrome-sandbox")))
}

/// True when unprivileged user namespaces, which Chromium's sandbox needs, are switched off or restricted.
fn userns_blocked() -> bool {
    let read = |p: &str| std::fs::read_to_string(p).ok().map(|s| s.trim().to_string());
    read("/proc/sys/kernel/apparmor_restrict_unprivileged_userns").as_deref() == Some("1")
        || read("/proc/sys/kernel/unprivileged_userns_clone").as_deref() == Some("0")
        || read("/proc/sys/user/max_user_namespaces").as_deref() == Some("0")
}

fn plan_appimage(p: &mut Plan, path: &Path, sys: &System) {
    let name = file_stem(path);
    p.name = name.clone();
    if sys.musl {
        p.warnings.push("This system uses musl libc; most AppImages are built for glibc and may not start.".into());
    }
    if !fuse2_present() {
        match sys.native {
            Some(n) => {
                let pkg = fuse2_package(n);
                p.warnings.push(format!("AppImages need FUSE 2 (`{pkg}`), which is missing. Unpack will install it."));
                bootstrap(p, sys, "Install FUSE 2 (needed by AppImage)", &[pkg]);
            }
            None => p.warnings.push("FUSE 2 may be missing; the AppImage may not start.".into()),
        }
    }
    let no_sandbox = is_electron(path) && userns_blocked();
    if no_sandbox {
        p.warnings.push("This app is built on Electron, and this system does not let ordinary programs create the user namespace that Chromium's sandbox needs (AppArmor `apparmor_restrict_unprivileged_userns`). It would crash at start. The menu entry will launch it with `--no-sandbox`, which turns that sandbox off for this app. Only continue if you trust it.".into());
    }
    p.step("Add to app menu", false, Action::IntegrateAppImage { src: path.to_path_buf(), name, no_sandbox });
    p.uninstall = Uninstall::Traced;
}

const RISKY: [(&str, &str); 9] = [
    ("rm -rf /", "contains `rm -rf /`, which can wipe the system"),
    ("mkfs", "formats disks (`mkfs`)"),
    ("dd if=", "uses `dd`, which can overwrite disks"),
    (":(){", "looks like a fork bomb"),
    ("chmod -R 777", "opens file permissions widely (`chmod -R 777`)"),
    ("/dev/sd", "writes to a disk device"),
    ("sudo ", "asks for admin rights (`sudo`)"),
    ("| sh", "downloads more code and runs it (`| sh`)"),
    ("| bash", "downloads more code and runs it (`| bash`)"),
];

pub fn scan_script(text: &str) -> Vec<String> {
    RISKY.iter().filter(|(n, _)| text.contains(n)).map(|(_, m)| format!("Script {m}.")).collect()
}

fn interpreter(text: &str) -> (String, Vec<String>) {
    let first = text.lines().next().unwrap_or("");
    let Some(rest) = first.strip_prefix("#!") else { return ("sh".into(), vec![]) };
    let mut w = rest.split_whitespace();
    let prog = w.next().unwrap_or("sh");
    if prog.ends_with("/env") {
        return ("env".into(), w.map(String::from).collect());
    }
    (prog.into(), w.map(String::from).collect())
}

fn plan_script(p: &mut Plan, path: &Path, kind: Kind) {
    p.name = file_stem(path);
    let bytes = std::fs::read(path).unwrap_or_default();
    let is_text = !bytes.starts_with(b"\x7fELF");
    let text = String::from_utf8_lossy(&bytes[..bytes.len().min(256 * 1024)]).to_string();
    if is_text {
        p.preview = Some(text.clone());
        p.warnings.extend(scan_script(&text));
        if text.contains("makeself") || text.contains("Makeself") {
            p.advice.push("Self-extracting installer (makeself). Drivers (NVIDIA etc.) should come from your distro instead.".into());
        }
    } else {
        p.warnings.push("This is a compiled installer; its contents can't be shown. Only run it if you trust the source.".into());
    }
    let (prog, mut args) = if is_text { interpreter(&text) } else { (path.to_string_lossy().to_string(), vec![]) };
    let root_hint = text.contains("sudo ") || text.contains("EUID") || text.contains("id -u");
    if root_hint {
        p.warnings.push("This installer seems to want admin rights. Tick 'Run as administrator' if it fails without them.".into());
    }
    let script = path.to_string_lossy().to_string();
    if is_text {
        args.push(script);
    }
    let label = match kind {
        Kind::RunInstaller => "Run installer",
        Kind::BinInstaller => "Run installer",
        _ => "Run script",
    };
    p.step(label, false, Action::Cmd { program: prog, args, cwd: path.parent().map(Path::to_path_buf) });
    p.root_optional = true;
    p.trace = true;
    p.uninstall = Uninstall::Traced;
}

// ---------------------------------------------------------------- archives and source

fn plan_archive(path: &Path, fmt: Archive, content: &Content, top: &str, members: &[String], work: &Path, sys: &System) -> Plan {
    let name = file_stem(path);
    let dest = work.join("x");
    let srcdir = if top.is_empty() { dest.clone() } else { dest.join(top) };
    let mut p = match content {
        Content::Wrapper(_) => {
            let mut p = Plan::new("archive", &name, "");
            p.refuse("This archive only wraps another installer.", &[]);
            p
        }
        Content::Plain => {
            let out = home().join("Unpacked").join(&name);
            let mut p = Plan::new("archive", &name, "");
            p.advice.push("This archive holds ordinary files, not an app. Unpack can extract it to a folder.".into());
            p.step("Extract files", false, Action::Extract { archive: path.to_path_buf(), fmt, dest: out.clone() });
            p.uninstall = Uninstall::Files { paths: vec![out.to_string_lossy().into()] };
            p
        }
        _ => {
            let mut p = plan_content(content, &srcdir, &name, sys);
            if p.supported {
                p.steps.insert(0, extract_step(path, fmt, &dest));
            }
            p
        }
    };
    p.kind = format!("archive/{fmt:?}").to_lowercase();
    p.preview = Some(members.iter().take(200).cloned().collect::<Vec<_>>().join("\n"));
    p
}

fn extract_step(path: &Path, fmt: Archive, dest: &Path) -> Step {
    let action = Action::Extract { archive: path.to_path_buf(), fmt, dest: dest.to_path_buf() };
    Step { label: "Extract archive".into(), display: format!("extract {} to a private work folder", path.display()), needs_root: false, action }
}

fn build_tools(sys: &System, p: &mut Plan, tools: &[&str], pkgs: &[&str]) {
    let missing: Vec<&str> = tools.iter().copied().filter(|t| !has(t)).collect();
    if missing.is_empty() { return; }
    if sys.immutable {
        p.refuse(&format!("Missing build tools: {}. Use a toolbox/distrobox container to build on this system.", missing.join(", ")), &[]);
        return;
    }
    p.warnings.push(format!("Missing build tools ({}); Unpack will install them.", missing.join(", ")));
    bootstrap(p, sys, "Install build tools", pkgs);
}

fn base_build_pkgs(sys: &System) -> Vec<&'static str> {
    match sys.native {
        Some(Native::Apt) => vec!["build-essential"],
        Some(Native::Dnf) => vec!["gcc", "gcc-c++", "make"],
        Some(Native::Zypper) => vec!["gcc", "gcc-c++", "make"],
        Some(Native::Pacman) => vec!["base-devel"],
        Some(Native::Apk) => vec!["build-base"],
        None => vec![],
    }
}

fn named_pkg(sys: &System, generic: &'static str) -> &'static str {
    match (generic, sys.native) {
        ("ninja", Some(Native::Apt | Native::Dnf)) => "ninja-build",
        ("python-pipx", Some(Native::Apt | Native::Dnf | Native::Zypper)) => "pipx",
        ("python-pipx", _) => "python-pipx",
        ("golang", Some(Native::Pacman)) => "go",
        ("golang", Some(Native::Zypper)) => "go",
        _ => generic,
    }
}

pub fn plan_content(content: &Content, dir: &Path, name: &str, sys: &System) -> Plan {
    let mut p = Plan::new("source", name, "");
    let prefix = home().join(".local").to_string_lossy().to_string();
    let d = Some(dir);
    let base = base_build_pkgs(sys);
    match content {
        Content::Source(b) => match b {
            Build::Autotools => {
                build_tools(sys, &mut p, &["make", "gcc"], &base);
                if !dir.join("configure").exists() {
                    build_tools(sys, &mut p, &["autoreconf"], &["autoconf", "automake", "libtool"]);
                    p.cmd("Generate configure script", false, "autoreconf", &["-fi"], d);
                }
                p.cmd("Configure", false, "./configure", &[&format!("--prefix={prefix}")], d);
                p.cmd("Compile", false, "make", &["-j4"], d);
                p.cmd("Install to ~/.local", false, "make", &["install"], d);
            }
            Build::CMake => {
                build_tools(sys, &mut p, &["make", "gcc"], &base);
                build_tools(sys, &mut p, &["cmake"], &["cmake"]);
                p.cmd("Configure", false, "cmake", &["-B", "build", &format!("-DCMAKE_INSTALL_PREFIX={prefix}"), "-DCMAKE_BUILD_TYPE=Release"], d);
                p.cmd("Compile", false, "cmake", &["--build", "build", "-j4"], d);
                p.cmd("Install to ~/.local", false, "cmake", &["--install", "build"], d);
            }
            Build::Meson => {
                build_tools(sys, &mut p, &["gcc"], &base);
                build_tools(sys, &mut p, &["meson", "ninja"], &["meson", named_pkg(sys, "ninja")]);
                p.cmd("Configure", false, "meson", &["setup", "build", &format!("--prefix={prefix}")], d);
                p.cmd("Install to ~/.local", false, "ninja", &["-C", "build", "install"], d);
            }
            Build::Cargo => {
                build_tools(sys, &mut p, &["cargo"], &["cargo"]);
                p.cmd("Build and install", false, "cargo", &["install", "--path", ".", "--root", &prefix], d);
            }
            Build::Go => {
                build_tools(sys, &mut p, &["go"], &[named_pkg(sys, "golang")]);
                let bin = format!("{prefix}/bin");
                p.cmd("Build and install", false, "env", &[&format!("GOBIN={bin}"), "go", "install", "./..."], d);
            }
            Build::Python => {
                build_tools(sys, &mut p, &["pipx"], &[named_pkg(sys, "python-pipx")]);
                p.cmd("Install with pipx", false, "pipx", &["install", "."], d);
            }
            Build::Node => {
                build_tools(sys, &mut p, &["npm"], &["npm"]);
                p.cmd("Install with npm", false, "npm", &["install", "-g", "--prefix", &prefix, "."], d);
            }
            Build::Make => {
                build_tools(sys, &mut p, &["make", "gcc"], &base);
                p.cmd("Compile", false, "make", &["-j4"], d);
                p.cmd("Install to ~/.local", false, "make", &["install", &format!("PREFIX={prefix}")], d);
            }
            Build::Pkgbuild => {
                if sys.native != Some(Native::Pacman) {
                    p.refuse("This source is an Arch PKGBUILD and needs Arch Linux or a derivative.", &[]);
                } else {
                    build_tools(sys, &mut p, &["makepkg"], &["base-devel"]);
                    p.cmd("Build package", false, "makepkg", &["-f", "--noconfirm"], d);
                    p.step("Install built package", true, Action::InstallBuilt { dirs: vec![dir.to_path_buf()], ext: "pkg.tar.zst".into() });
                    p.uninstall = Uninstall::Manual { note: "set after install".into() };
                }
            }
            Build::DebianDir => {
                if sys.native != Some(Native::Apt) {
                    p.refuse("This source has a Debian packaging folder and needs a Debian-based system.", &[]);
                } else {
                    build_tools(sys, &mut p, &["dpkg-buildpackage", "fakeroot"], &["dpkg-dev", "build-essential", "fakeroot"]);
                    p.cmd("Install build dependencies", true, "env", &["DEBIAN_FRONTEND=noninteractive", "apt-get", "build-dep", "-y", "./"], d);
                    p.cmd("Build package", false, "dpkg-buildpackage", &["-us", "-uc", "-b"], d);
                    let parent = dir.parent().unwrap_or(dir).to_path_buf();
                    p.step("Install built package", true, Action::InstallBuilt { dirs: vec![parent], ext: "deb".into() });
                }
            }
            Build::RpmSpec => p.refuse(
                "This source has an RPM .spec file. Building from a spec needs extra source files.",
                &["Look for a ready-made .rpm or Flatpak, or build with `rpmbuild` manually."],
            ),
        },
        Content::Installer(script) => {
            p.root_optional = true;
            let path = dir.join(script);
            p.step("Run installer", false, Action::Cmd { program: "sh".into(), args: vec![path.to_string_lossy().into()], cwd: Some(dir.to_path_buf()) });
            p.preview = std::fs::read_to_string(&path).ok();
            if let Some(t) = &p.preview { p.warnings.extend(scan_script(t)); }
        }
        Content::Portable => {
            p.step("Install as portable app", false, Action::InstallPortable { from: dir.to_path_buf(), name: name.to_string() });
        }
        Content::Wrapper(_) | Content::Plain => p.refuse("No known way to install this.", &[]),
    }
    if matches!(content, Content::Source(Build::Pkgbuild | Build::DebianDir)) {
        // native package manager owns removal; runner fills the real ident
        p.trace = false;
    } else {
        p.trace = true;
        p.uninstall = Uninstall::Traced;
    }
    p
}

fn plan_dsc(path: &Path, work: &Path, sys: &System) -> Plan {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let name = field(&text, "Source:").unwrap_or_else(|| file_stem(path));
    let mut p = Plan::new("dsc", &name, "");
    p.version = field(&text, "Version:");
    if sys.native != Some(Native::Apt) {
        p.refuse("Debian source packages (.dsc) can only be built on Debian-based systems.", &["Look for a ready-made package or Flatpak."]);
        return p;
    }
    let sums = dsc_sums(&text);
    let dir = path.parent().unwrap_or(Path::new("."));
    let mut bad = vec![];
    for (hash, file) in &sums {
        match crate::fetch::sha256_file(&dir.join(file)) {
            Ok(h) if h == *hash => {}
            Ok(_) => bad.push(format!("{file} does not match its checksum")),
            Err(_) => bad.push(format!("{file} is missing next to the .dsc")),
        }
    }
    if !bad.is_empty() {
        p.refuse(&format!("Source files are incomplete or corrupt: {}.", bad.join("; ")), &["Download every file listed in the .dsc into the same folder."]);
        return p;
    }
    let src = work.join("src");
    let ver = p.version.clone().unwrap_or_default();
    p.warnings.push(format!("Building {name} {ver} from source can take several minutes."));
    build_tools(sys, &mut p, &["dpkg-source", "dpkg-buildpackage", "fakeroot"], &["dpkg-dev", "build-essential", "fakeroot"]);
    p.cmd("Unpack source", false, "dpkg-source", &["-x", &path.to_string_lossy(), &src.to_string_lossy()], Some(dir));
    p.cmd("Install build dependencies", true, "env", &["DEBIAN_FRONTEND=noninteractive", "apt-get", "build-dep", "-y", "./"], Some(&src));
    p.cmd("Build package", false, "dpkg-buildpackage", &["-us", "-uc", "-b"], Some(&src));
    p.step("Install built package", true, Action::InstallBuilt { dirs: vec![work.to_path_buf()], ext: "deb".into() });
    p
}

/// `(sha256, filename)` from a .dsc `Checksums-Sha256:` block.
pub fn dsc_sums(text: &str) -> Vec<(String, String)> {
    let mut out = vec![];
    let mut inside = false;
    for l in text.lines() {
        if l.starts_with("Checksums-Sha256:") {
            inside = true;
        } else if inside && l.starts_with(' ') {
            let mut w = l.split_whitespace();
            if let (Some(h), Some(_), Some(f)) = (w.next(), w.next(), w.next()) {
                out.push((h.to_string(), f.to_string()));
            }
        } else if inside {
            break;
        }
    }
    out
}

/// A package or app id that is safe to hand to a package manager as one argument.
pub fn safe_ident(ident: &str) -> bool {
    !ident.is_empty() && ident.len() < 256 && !ident.starts_with('-') && ident.chars().all(|c| c.is_ascii_alphanumeric() || "+._:@-".contains(c))
}

/// Install `ident` from a configured source: `flathub`, `snap`, or the native manager's name (`apt`, `dnf`…).
pub fn plan_repo(source: &str, ident: &str, sys: &System) -> Plan {
    let mut p = Plan::new(source, ident, &format!("{source}:{ident}"));
    if !safe_ident(ident) {
        p.refuse(&format!("Refusing unsafe package name: {ident:?}"), &[]);
        return p;
    }
    match source {
        "flathub" => {
            if !has("flatpak") {
                bootstrap(&mut p, sys, "Install Flatpak", &["flatpak"]);
                if !p.supported { return p; }
            }
            p.cmd("Add Flathub", false, "flatpak", &["remote-add", "--user", "--if-not-exists", "flathub", "https://dl.flathub.org/repo/flathub.flatpakrepo"], None);
            p.cmd("Install from Flathub", false, "flatpak", &["install", "--user", "-y", "--noninteractive", "flathub", ident], None);
            p.uninstall = Uninstall::Manager { manager: "flatpak".into(), ident: ident.into(), user: true };
        }
        "snap" => {
            if !has("snap") {
                p.refuse("Snap support (snapd) isn't installed.", &["Install the Flathub version instead, or install snapd first."]);
                return p;
            }
            let classic = capture("snap", &["info", ident]).is_ok_and(|i| i.lines().any(|l| l.contains(" classic")));
            let mut args = vec!["install", ident];
            if classic {
                p.warnings.push("This snap uses classic confinement (full system access).".into());
                args.push("--classic");
            }
            p.cmd("Install from the Snap Store", true, "snap", &args, None);
            p.uninstall = Uninstall::Manager { manager: "snap".into(), ident: ident.into(), user: false };
        }
        _ => match sys.native.filter(|n| manager_name(*n) == source) {
            Some(n) => {
                let (prog, args) = install_pkgs(n, &[ident]);
                p.step("Install from your distro's repositories", true, Action::Cmd { program: prog, args, cwd: None });
                p.uninstall = Uninstall::Manager { manager: source.into(), ident: ident.into(), user: false };
            }
            None => p.refuse(&format!("{source} is not this system's package manager."), &[]),
        },
    }
    if p.supported && sys.immutable && source != "flathub" {
        p.warnings.push("This system has a read-only base. System-wide package installs may need a reboot or may be undone by updates; Flatpak is safer.".into());
    }
    p
}

/// The name records use for a native manager: `apt`, `dnf`…
pub fn manager_name(n: Native) -> &'static str {
    match n {
        Native::Apt => "apt",
        Native::Dnf => "dnf",
        Native::Zypper => "zypper",
        Native::Pacman => "pacman",
        Native::Apk => "apk",
    }
}

/// Removal command for a package-manager-owned install.
pub fn removal(manager: &str, ident: &str, user: bool) -> Result<(String, Vec<String>, bool), String> {
    if !safe_ident(ident) {
        return Err(format!("Refusing unsafe package name: {ident:?}"));
    }
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    let (prog, args, root) = match manager {
        "apt" => ("env", s(&["DEBIAN_FRONTEND=noninteractive", "apt-get", "remove", "-y", ident]), true),
        "dnf" => (if has("dnf") { "dnf" } else { "yum" }, s(&["remove", "-y", ident]), true),
        "zypper" => ("zypper", s(&["--non-interactive", "remove", ident]), true),
        "pacman" => ("pacman", s(&["-R", "--noconfirm", ident]), true),
        "apk" => ("apk", s(&["del", ident]), true),
        "snap" => ("snap", s(&["remove", ident]), true),
        "flatpak" => ("flatpak", s(&["uninstall", if user { "--user" } else { "--system" }, "-y", ident]), !user),
        "flatpak-remote" => ("flatpak", s(&["remote-delete", "--user", ident]), false),
        other => return Err(format!("Unknown package manager: {other}")),
    };
    Ok((prog.into(), args, root))
}

#[allow(dead_code)]
pub fn family_hint(f: Family) -> &'static str {
    match f {
        Family::Debian => ".deb",
        Family::RedHat | Family::Suse => ".rpm",
        Family::Arch => ".pkg.tar.zst",
        Family::Alpine => ".apk",
        _ => "universal formats",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appimage_offset_is_the_end_of_the_section_headers() {
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join("a.AppImage");
        let mut h = vec![0u8; 64];
        h[..4].copy_from_slice(b"\x7fELF");
        h[4] = 2;
        h[0x28..0x30].copy_from_slice(&1000u64.to_le_bytes());
        h[0x3A..0x3C].copy_from_slice(&64u16.to_le_bytes());
        h[0x3C..0x3E].copy_from_slice(&5u16.to_le_bytes());
        std::fs::write(&f, &h).unwrap();
        assert_eq!(appimage_offset(&f), Some(1000 + 64 * 5));
        std::fs::write(&f, b"not an elf").unwrap();
        assert_eq!(appimage_offset(&f), None);
    }

    use crate::system::detect;

    fn fake(native: Option<Native>) -> System {
        let mut s = detect();
        s.native = native;
        s.immutable = false;
        s
    }

    #[test]
    fn scans_risky_scripts() {
        assert!(scan_script("echo hi").is_empty());
        let w = scan_script("sudo rm -rf / ; curl x | sh");
        assert!(w.len() >= 3);
    }

    #[test]
    fn dsc_checksums() {
        let t = "Source: foo\nVersion: 1.0-1\nChecksums-Sha1:\n aa 1 foo.orig.tar.gz\nChecksums-Sha256:\n abc 10 foo_1.0.orig.tar.gz\n def 20 foo_1.0-1.debian.tar.xz\nFiles:\n x 1 y\n";
        assert_eq!(dsc_sums(t), vec![("abc".into(), "foo_1.0.orig.tar.gz".into()), ("def".into(), "foo_1.0-1.debian.tar.xz".into())]);
    }

    #[test]
    fn stems() {
        assert_eq!(file_stem(Path::new("/a/App-1.2.AppImage")), "App-1.2");
        assert_eq!(file_stem(Path::new("/a/x.tar.gz")), "x");
        assert_eq!(file_stem(Path::new("/a/pkg-1-1-x86_64.pkg.tar.zst")), "pkg-1-1-x86_64");
    }

    #[test]
    fn removal_commands() {
        let (p, a, root) = removal("pacman", "vlc", false).unwrap();
        assert_eq!((p.as_str(), root), ("pacman", true));
        assert_eq!(a, ["-R", "--noconfirm", "vlc"]);
        assert!(removal("apt", "--purge", false).is_err());
        assert!(removal("apt", "a b", false).is_err());
        assert!(removal("rm", "x", false).is_err());
        let (_, a, root) = removal("flatpak", "org.x.App", true).unwrap();
        assert!(!root && a.contains(&"--user".to_string()));
    }

    #[test]
    fn foreign_packages_refused() {
        let sys = fake(Some(Native::Pacman));
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join("a.deb");
        std::fs::write(&f, b"!<arch>\ndebian-binary").unwrap();
        let p = plan_package(&f, Kind::Deb, &sys);
        assert!(!p.supported);
        assert!(p.steps.is_empty());
    }

    #[test]
    fn android_apk_refused() {
        let p = plan_package(Path::new("x.apk"), Kind::AndroidApk, &fake(Some(Native::Apk)));
        assert!(!p.supported);
    }

    #[test]
    fn cargo_source_plan_uses_local_prefix() {
        let sys = fake(Some(Native::Apt));
        let p = plan_content(&Content::Source(Build::Cargo), Path::new("/tmp/x"), "x", &sys);
        let has_prefix = p.steps.iter().any(|s| s.display.contains(".local"));
        assert!(has_prefix || !has("cargo"));
        assert!(p.trace);
    }

    #[test]
    fn script_plan_shows_preview_and_traces() {
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join("install.sh");
        std::fs::write(&f, "#!/bin/bash\necho hi\n").unwrap();
        let p = plan_package(&f, Kind::Script, &fake(None));
        assert!(p.trace && p.root_optional);
        assert!(p.preview.unwrap().contains("echo hi"));
        assert!(p.steps[0].display.starts_with("/bin/bash"));
    }
}
