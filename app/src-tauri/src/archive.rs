use crate::sniff::Archive;
use crate::util::{capture, has};
use serde::Serialize;
use std::path::{Component, Path};

pub const MAX_MEMBERS: usize = 200_000;

/// List member paths without extracting.
pub fn list(path: &Path, fmt: Archive) -> Result<Vec<String>, String> {
    let p = path.to_string_lossy().to_string();
    let attempts: Vec<(&str, Vec<&str>)> = match fmt {
        Archive::SevenZ => vec![("7z", vec!["l", "-slt", "-ba", &p]), ("7zz", vec!["l", "-slt", "-ba", &p]), ("bsdtar", vec!["-tf", &p])],
        Archive::Zip => vec![("unzip", vec!["-Z1", &p]), ("bsdtar", vec!["-tf", &p]), ("7z", vec!["l", "-slt", "-ba", &p])],
        _ => vec![("tar", vec!["-tf", &p]), ("bsdtar", vec!["-tf", &p])],
    };
    let mut last = format!("No tool available to read {fmt:?} archives");
    for (prog, args) in attempts {
        if !has(prog) {
            continue;
        }
        match capture(prog, &args) {
            Ok(out) => {
                let seven = prog.starts_with("7z");
                return Ok(out
                    .lines()
                    .filter_map(|l| if seven { l.strip_prefix("Path = ") } else { Some(l) })
                    .map(str::to_string)
                    .collect());
            }
            Err(e) => last = e,
        }
    }
    Err(last)
}

/// Reject absolute paths, `..` traversal and absurd member counts (zip-slip, bombs).
pub fn check_members(members: &[String]) -> Result<(), String> {
    if members.len() > MAX_MEMBERS {
        return Err(format!("Archive has {} files; refusing (limit {MAX_MEMBERS})", members.len()));
    }
    for m in members {
        let p = Path::new(m);
        if p.is_absolute() || p.components().any(|c| matches!(c, Component::ParentDir | Component::Prefix(_))) {
            return Err(format!("Unsafe path inside archive: {m}"));
        }
    }
    Ok(())
}

/// Extract into `dest` (must exist). Members are checked first by the caller.
pub fn extract(path: &Path, fmt: Archive, dest: &Path) -> Result<(), String> {
    let (p, d) = (path.to_string_lossy().to_string(), dest.to_string_lossy().to_string());
    let attempts: Vec<(&str, Vec<&str>)> = match fmt {
        Archive::SevenZ => vec![("7z", vec!["x", "-y", "-bd"]), ("7zz", vec!["x", "-y", "-bd"])],
        Archive::Zip => vec![("unzip", vec!["-q", "-o"]), ("bsdtar", vec!["-xf"])],
        Archive::Gz | Archive::Xz | Archive::Bz2 | Archive::Zst => return decompress_single(path, fmt, dest),
        _ => vec![("tar", vec!["-xf"]), ("bsdtar", vec!["-xf"])],
    };
    let mut last = format!("No tool available to extract {fmt:?} archives");
    for (prog, pre) in attempts {
        if !has(prog) {
            continue;
        }
        let mut cmd = crate::util::command(prog);
        cmd.args(&pre);
        match (prog, fmt) {
            ("7z" | "7zz", _) => cmd.arg(format!("-o{d}")).arg(&p),
            ("unzip", _) => cmd.arg(&p).arg("-d").arg(&d),
            _ => cmd.arg(&p).arg("-C").arg(&d),
        };
        let out = cmd.output().map_err(|e| e.to_string())?;
        if out.status.success() {
            return verify_tree(dest);
        }
        last = String::from_utf8_lossy(&out.stderr).trim().to_string();
    }
    Err(last)
}

fn decompress_single(path: &Path, fmt: Archive, dest: &Path) -> Result<(), String> {
    let (prog, strip) = match fmt {
        Archive::Gz => ("gzip", ".gz"),
        Archive::Xz => ("xz", ".xz"),
        Archive::Bz2 => ("bzip2", ".bz2"),
        _ => ("zstd", ".zst"),
    };
    if !has(prog) {
        return Err(format!("`{prog}` is not installed"));
    }
    let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
    let out_name = name.strip_suffix(strip).filter(|s| !s.is_empty()).unwrap_or("decompressed");
    let out = std::fs::File::create(dest.join(out_name)).map_err(|e| e.to_string())?;
    let st = crate::util::command(prog).args(["-dc"]).arg(path).stdout(out).status().map_err(|e| e.to_string())?;
    if st.success() { Ok(()) } else { Err(format!("{prog} failed")) }
}

/// After extraction no entry may resolve outside `root` (symlink escapes).
pub fn verify_tree(root: &Path) -> Result<(), String> {
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    for e in walkdir::WalkDir::new(&root).min_depth(1).follow_links(false) {
        let e = e.map_err(|e| e.to_string())?;
        let parent = e.path().parent().unwrap_or(&root).canonicalize().map_err(|e| e.to_string())?;
        if !parent.starts_with(&root) {
            return Err(format!("Archive escaped its folder: {}", e.path().display()));
        }
    }
    Ok(())
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case", tag = "type", content = "value")]
pub enum Build {
    Autotools,
    CMake,
    Meson,
    Cargo,
    Go,
    Python,
    Node,
    Make,
    Pkgbuild,
    DebianDir,
    RpmSpec,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case", tag = "type", content = "value")]
pub enum Content {
    Source(Build),
    Installer(String),
    Portable,
    Wrapper(String),
    Plain,
}

/// Strip a single shared top-level directory, returning (prefix, relative paths).
pub fn top_dir(members: &[String]) -> (String, Vec<String>) {
    let first = |m: &str| m.trim_start_matches("./").split('/').next().unwrap_or("").to_string();
    let Some(top) = members.iter().map(|m| first(m)).find(|t| !t.is_empty()) else {
        return (String::new(), vec![]);
    };
    let all_under = members.iter().all(|m| {
        let m = m.trim_start_matches("./");
        m.is_empty() || m == top || m.starts_with(&format!("{top}/"))
    });
    let rel = |m: &String| {
        let m = m.trim_start_matches("./");
        if all_under { m.strip_prefix(&format!("{top}/")).unwrap_or("").to_string() } else { m.to_string() }
    };
    let rels = members.iter().map(rel).filter(|s| !s.is_empty()).collect();
    (if all_under { top } else { String::new() }, rels)
}

/// Decide what an archive holds from its (top-dir-stripped) member list.
/// Native packaging recipes win over generic builds because the package manager then owns removal.
pub fn classify(rel: &[String]) -> Content {
    let at_root = |n: &str| rel.iter().any(|m| m == n);
    let root_ext = |ext: &str| rel.iter().any(|m| !m.contains('/') && m.ends_with(ext));
    if let Some(w) = rel.iter().find(|m| {
        let l = m.to_lowercase();
        !m.contains('/') && (l.ends_with(".deb") || l.ends_with(".rpm") || l.ends_with(".appimage") || l.ends_with(".snap") || l.ends_with(".flatpak"))
    }) {
        return Content::Wrapper(w.clone());
    }
    if at_root("PKGBUILD") { return Content::Source(Build::Pkgbuild); }
    if rel.iter().any(|m| m == "debian/control") { return Content::Source(Build::DebianDir); }
    if root_ext(".spec") { return Content::Source(Build::RpmSpec); }
    if at_root("configure") || at_root("configure.ac") || at_root("autogen.sh") { return Content::Source(Build::Autotools); }
    if at_root("CMakeLists.txt") { return Content::Source(Build::CMake); }
    if at_root("meson.build") { return Content::Source(Build::Meson); }
    if at_root("Cargo.toml") { return Content::Source(Build::Cargo); }
    if at_root("go.mod") { return Content::Source(Build::Go); }
    if at_root("pyproject.toml") || at_root("setup.py") { return Content::Source(Build::Python); }
    if at_root("package.json") { return Content::Source(Build::Node); }
    if at_root("Makefile") || at_root("makefile") { return Content::Source(Build::Make); }
    if let Some(i) = rel.iter().find(|m| {
        !m.contains('/') && matches!(m.to_lowercase().as_str(), "install.sh" | "setup.sh" | "install" | "installer.sh") || (!m.contains('/') && m.ends_with(".run"))
    }) {
        return Content::Installer(i.clone());
    }
    if rel.iter().any(|m| m.ends_with(".desktop") || m.starts_with("bin/")) || rel.iter().any(|m| !m.contains('/') && !m.contains('.') && !m.is_empty()) {
        return Content::Portable;
    }
    Content::Plain
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn v(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn rejects_traversal() {
        assert!(check_members(&v(&["a/b", "a/c"])).is_ok());
        assert!(check_members(&v(&["../evil"])).is_err());
        assert!(check_members(&v(&["a/../../evil"])).is_err());
        assert!(check_members(&v(&["/etc/passwd"])).is_err());
    }

    #[test]
    fn strips_top_dir() {
        let (top, rel) = top_dir(&v(&["proj-1.0/", "proj-1.0/configure", "proj-1.0/src/a.c"]));
        assert_eq!(top, "proj-1.0");
        assert_eq!(rel, v(&["configure", "src/a.c"]));
        let (top, rel) = top_dir(&v(&["a", "b/c"]));
        assert_eq!(top, "");
        assert_eq!(rel.len(), 2);
    }

    #[test]
    fn classifies() {
        assert_eq!(classify(&v(&["configure", "src/a.c"])), Content::Source(Build::Autotools));
        assert_eq!(classify(&v(&["CMakeLists.txt"])), Content::Source(Build::CMake));
        assert_eq!(classify(&v(&["PKGBUILD", "Makefile"])), Content::Source(Build::Pkgbuild));
        assert_eq!(classify(&v(&["debian/control", "configure"])), Content::Source(Build::DebianDir));
        assert_eq!(classify(&v(&["install.sh", "data/x"])), Content::Installer("install.sh".into()));
        assert_eq!(classify(&v(&["bin/app", "lib/x.so"])), Content::Portable);
        assert_eq!(classify(&v(&["app.deb"])), Content::Wrapper("app.deb".into()));
        assert_eq!(classify(&v(&["readme.txt", "notes.md"])), Content::Plain);
    }

    #[test]
    fn extracts_and_lists_tar() {
        if !has("tar") { return; }
        let d = tempfile::tempdir().unwrap();
        let src = d.path().join("src");
        std::fs::create_dir_all(src.join("p")).unwrap();
        std::fs::write(src.join("p/configure"), "x").unwrap();
        let tgz = d.path().join("p.tar.gz");
        let ok = Command::new("tar").args(["-czf"]).arg(&tgz).arg("-C").arg(&src).arg("p").status().unwrap().success();
        assert!(ok);
        let m = list(&tgz, Archive::TarGz).unwrap();
        assert!(m.iter().any(|x| x.ends_with("configure")));
        let out = d.path().join("out");
        std::fs::create_dir(&out).unwrap();
        extract(&tgz, Archive::TarGz, &out).unwrap();
        assert!(out.join("p/configure").exists());
    }
}
