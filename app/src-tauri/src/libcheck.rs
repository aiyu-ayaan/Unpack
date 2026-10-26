//! After a native install: can the installed programs find every shared library they need?
//! A `.deb` that forgets a dependency installs fine and then fails to start; this says so.
use crate::util::{capture, command};
use std::io::Read;
use std::path::PathBuf;

/// Files the package manager says belong to `ident`, or none when it cannot say.
fn package_files(manager: &str, ident: &str) -> Vec<PathBuf> {
    let (prog, args): (&str, Vec<&str>) = match manager {
        "apt" => ("dpkg", vec!["-L", ident]),
        "dnf" | "yum" | "zypper" => ("rpm", vec!["-ql", ident]),
        "pacman" => ("pacman", vec!["-Ql", ident]),
        "apk" => ("apk", vec!["info", "-L", ident]),
        _ => return vec![],
    };
    let Ok(out) = capture(prog, &args) else { return vec![] };
    out.lines()
        .filter_map(|l| if manager == "pacman" { l.split_once(' ').map(|x| x.1) } else { Some(l) })
        .map(|l| if l.starts_with('/') { PathBuf::from(l) } else { PathBuf::from(format!("/{l}")) })
        .filter(|p| std::fs::symlink_metadata(p).is_ok_and(|m| m.is_file()) && is_elf(p))
        .take(4000)
        .collect()
}

fn is_elf(p: &PathBuf) -> bool {
    let mut head = [0u8; 4];
    std::fs::File::open(p).and_then(|mut f| f.read_exact(&mut head)).is_ok() && &head == b"\x7fELF"
}

/// Library names in `ldd` output that the loader could not find.
pub fn not_found(ldd_output: &str) -> Vec<String> {
    let mut v: Vec<String> = ldd_output.lines().filter_map(|l| l.trim().strip_suffix(" => not found")).map(str::to_string).collect();
    v.sort();
    v.dedup();
    v
}

/// Sonames the installed files of `ident` need but the system lacks.
pub fn missing(manager: &str, ident: &str) -> Vec<String> {
    let files = package_files(manager, ident);
    let mut all = vec![];
    for chunk in files.chunks(100) {
        if let Ok(o) = command("ldd").args(chunk).env("LC_ALL", "C").output() {
            all.extend(not_found(&String::from_utf8_lossy(&o.stdout)));
        }
    }
    all.sort();
    all.dedup();
    all
}

/// Debian package that usually ships `libxcb-cursor.so.0`: `libxcb-cursor0`. Only returned when apt knows it.
fn apt_package(soname: &str) -> Option<String> {
    let (name, rest) = soname.split_once(".so.")?;
    let ver = rest.split('.').next().filter(|v| !v.is_empty() && v.chars().all(|c| c.is_ascii_digit()))?;
    let pkg = format!("{name}{ver}");
    capture("apt-cache", &["show", "--no-all-versions", &pkg]).ok().map(|_| pkg)
}

/// Check a fresh native install. Logs each problem and returns one note for the result, or none.
pub fn report(manager: &str, ident: &str, emit: &dyn Fn(&str, &str)) -> Option<String> {
    let miss = missing(manager, ident);
    if miss.is_empty() {
        return None;
    }
    let fixes: Vec<String> = if manager == "apt" { miss.iter().filter_map(|m| apt_package(m)).collect() } else { vec![] };
    let mut note = format!("{ident} is installed but may not start: these libraries are missing: {}.", miss.join(", "));
    if !fixes.is_empty() {
        note.push_str(&format!(" Try: sudo apt install {}", fixes.join(" ")));
    }
    emit("warn", &note);
    Some(note)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ldd_not_found_lines() {
        let out = "\tlinux-vdso.so.1 (0x7ffc)\n\tlibxcb-cursor.so.0 => not found\n\tlibc.so.6 => /lib/x86_64-linux-gnu/libc.so.6 (0x7f)\n\tlibxcb-cursor.so.0 => not found\n\tlibtiff.so.5 => not found\n";
        assert_eq!(not_found(out), vec!["libtiff.so.5".to_string(), "libxcb-cursor.so.0".to_string()]);
        assert!(not_found("\tnot a dynamic executable\n").is_empty());
    }
}
