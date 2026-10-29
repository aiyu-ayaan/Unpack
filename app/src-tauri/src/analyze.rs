//! Input string → Plan. Fetches or inspects, never executes the artifact.
use crate::archive::{self, Content};
use crate::input::{self, Source};
use crate::plan::{self, Input, Meta, Plan};
use crate::sniff::{sniff, Archive, Kind};
use crate::system::System;
use crate::fetch::{self, Progress, CANCELLED};
use crate::util::{cache_dir, capture_until, has, now};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

pub fn new_id() -> String {
    format!("{:x}-{:x}", now(), COUNTER.fetch_add(1, Ordering::Relaxed))
}

fn work_dir(id: &str) -> Result<PathBuf, String> {
    let d = cache_dir().join("work").join(id);
    std::fs::create_dir_all(&d).map_err(|e| e.to_string())?;
    Ok(d)
}

/// Delete work folders older than a day. Called at startup.
pub fn clean_old_work() {
    let root = cache_dir().join("work");
    let Ok(rd) = std::fs::read_dir(&root) else { return };
    for e in rd.flatten() {
        let old = e.metadata().and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|a| a.as_secs() > 86_400);
        if old {
            let _ = std::fs::remove_dir_all(e.path());
        }
    }
}

/// `progress` reports download bytes and is asked between stages; returning false cancels. A cancelled or
/// failed analyze removes its work folder.
/// `as_package`: build source into a native package instead of installing into `~/.local` (see `Plan::can_package`).
pub fn analyze(text: &str, sys: &System, allow_http: bool, as_package: bool, progress: Progress) -> Result<Plan, String> {
    let id = new_id();
    let work = work_dir(&id)?;
    let r = analyze_in(text, sys, allow_http, as_package, progress, &work).and_then(|p| if progress(0, None) { Ok(p) } else { Err(CANCELLED.into()) });
    match r {
        Ok(mut plan) => {
            plan.id = id;
            crate::deps::preview(&mut plan);
            crate::library::duplicate_warning(&mut plan, sys);
            Ok(plan)
        }
        Err(e) => {
            let _ = std::fs::remove_dir_all(&work);
            Err(e)
        }
    }
}

fn analyze_in(text: &str, sys: &System, allow_http: bool, as_package: bool, progress: Progress, work: &Path) -> Result<Plan, String> {
    match input::parse(text)? {
        Source::File(p) => {
            let sha = fetch::sha256_file(&p).map_err(|e| format!("Can't read {}: {e}", p.display()))?;
            from_file(&p, sys, work, Meta { source: p.display().to_string(), sha256: Some(sha), pipe_to_shell: false, as_package }, 0)
        }
        Source::Url { url, pipe_to_shell } => {
            let d = fetch::download(&url, work, allow_http, progress)?;
            // The record keeps the link as given (updates read the project from it) and where it really came from.
            let mut plan = from_file(&d.path, sys, work, Meta { source: url.clone(), sha256: Some(d.sha256), pipe_to_shell, as_package }, if pipe_to_shell { 100 } else { 0 })?;
            plan.fetched = (d.final_url != url).then_some(d.final_url);
            Ok(plan)
        }
        Source::Git { url } => from_git(&url, sys, work, Meta { source: url.clone(), sha256: None, pipe_to_shell: false, as_package }, progress),
    }
}

/// `depth` ≥ 100 means "treat unknown text as a script" (pipe-to-shell input).
fn from_file(path: &Path, sys: &System, work: &Path, meta: Meta, depth: u32) -> Result<Plan, String> {
    let force_script = depth >= 100;
    let depth = depth % 100;
    if depth > 3 {
        return Err("Archive is nested too deeply".into());
    }
    // One part of a Debian source package: open the .dsc next to it, which names every part.
    let part = path.file_name().filter(|_| depth == 0).and_then(|n| debian_part(&n.to_string_lossy()));
    if let Some((upstream, debian)) = &part {
        if let Some(dsc) = sibling_dsc(path, upstream) {
            let sha = fetch::sha256_file(&dsc).map_err(|e| e.to_string())?;
            let mut plan = from_file(&dsc, sys, work, Meta { source: dsc.display().to_string(), sha256: Some(sha), ..meta }, 1)?;
            let n = dsc.file_name().unwrap_or_default().to_string_lossy();
            plan.advice.insert(0, format!("This file is one part of a Debian source package. Unpack opened {n} next to it, which lists every part."));
            return Ok(plan);
        }
        if *debian {
            let mut p = Plan::new("dsc", &plan::file_stem(path), &meta.source);
            p.refuse(
                "This is only the Debian packaging part of a source package, not something to install by itself.",
                &["Download the package's .dsc file and the .orig.tar file it lists into the same folder, then open the .dsc."],
            );
            return Ok(p);
        }
    }
    let mut kind = sniff(path).map_err(|e| e.to_string())?;
    if kind == Kind::Unknown && force_script {
        kind = Kind::Script;
    }
    let verify = meta.sha256.clone().map(|h| (path.to_path_buf(), h));
    let mut plan = match kind {
        Kind::Dsc => plan::build(&Input::Dsc { path: path.to_path_buf(), work: work.to_path_buf() }, sys, &meta),
        Kind::Archive(fmt) if matches!(fmt, Archive::Gz | Archive::Xz | Archive::Bz2 | Archive::Zst) => {
            let out = work.join(format!("d{depth}"));
            std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
            archive::extract(path, fmt, &out)?;
            let inner = std::fs::read_dir(&out).map_err(|e| e.to_string())?.flatten().next().ok_or("Compressed file is empty")?.path();
            return from_file(&inner, sys, work, meta, depth + 1);
        }
        Kind::Archive(fmt) => {
            let members = archive::list(path, fmt)?;
            archive::check_members(&members)?;
            let (top, rel) = archive::top_dir(&members);
            let content = archive::classify(&rel);
            if let Content::Wrapper(inner) = &content {
                let out = work.join(format!("w{depth}"));
                std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
                archive::extract(path, fmt, &out)?;
                archive::verify_tree(&out)?;
                let f = if top.is_empty() { out.join(inner) } else { out.join(&top).join(inner) };
                let h = fetch::sha256_file(&f).map_err(|e| e.to_string())?;
                return from_file(&f, sys, work, Meta { sha256: Some(h), ..meta }, depth + 1);
            }
            plan::build(&Input::Archive { path: path.to_path_buf(), fmt, content, top, members: rel, work: work.to_path_buf() }, sys, &meta)
        }
        k => plan::build(&Input::Package { path: path.to_path_buf(), kind: k }, sys, &meta),
    };
    plan.verify = verify;
    if part.is_some() && plan.supported {
        plan.warnings.push("This is the upstream source of a Debian source package. Unpack will build it as plain source; to build the Debian package instead, put its .dsc and .debian.tar file next to it and open the .dsc.".into());
    }
    Ok(plan)
}

/// `(name_upstreamversion, is_debian_tar)` for a part of a Debian source package: `foo_1.2.orig.tar.gz`,
/// `foo_1.2.orig-docs.tar.xz` or `foo_1.2-3.debian.tar.xz` (whose .dsc is `foo_1.2-3.dsc`).
pub fn debian_part(name: &str) -> Option<(String, bool)> {
    let (stem, debian) = match (name.find(".orig.tar.").or_else(|| name.find(".orig-").filter(|_| name.contains(".tar."))), name.find(".debian.tar.")) {
        (Some(i), _) => (&name[..i], false),
        (None, Some(i)) => (&name[..i], true),
        _ => return None,
    };
    if !stem.contains('_') {
        return None;
    }
    let upstream = if debian { stem.rsplit_once('-').map_or(stem, |x| x.0) } else { stem };
    Some((upstream.to_string(), debian))
}

/// The newest `.dsc` beside `path` for that upstream version (`foo_1.2-3.dsc` for `foo_1.2`).
fn sibling_dsc(path: &Path, upstream: &str) -> Option<PathBuf> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(path.parent()?)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()).is_some_and(|n| n.ends_with(".dsc") && n.strip_prefix(upstream).is_some_and(|r| r.starts_with(['-', '.']))))
        .collect();
    found.sort();
    found.pop()
}

fn from_git(url: &str, sys: &System, work: &Path, meta: Meta, progress: Progress) -> Result<Plan, String> {
    if !has("git") {
        return Err("`git` is not installed. Install it first (for example: sudo apt install git).".into());
    }
    let dest = work.join("repo");
    capture_until("git", &["clone", "--depth", "1", "--", url, &dest.to_string_lossy()], &|| progress(0, None))?;
    let files: Vec<String> = walkdir::WalkDir::new(&dest)
        .max_depth(2)
        .into_iter()
        .flatten()
        .filter(|e| !e.path().components().any(|c| c.as_os_str() == ".git"))
        .filter_map(|e| e.path().strip_prefix(&dest).ok().map(|p| p.to_string_lossy().into_owned()))
        .filter(|s| !s.is_empty())
        .collect();
    let content = archive::classify(&files);
    let mut plan = plan::build(&Input::Repo { dir: dest.clone(), content }, sys, &meta);
    let name = url.trim_end_matches('/').rsplit(['/', ':']).next().unwrap_or("repo").trim_end_matches(".git").to_string();
    plan.name = name;
    plan.kind = "git".into();
    if plan.supported {
        plan.warnings.push("Source code from the internet will be compiled and run. Only continue for projects you trust.".into());
    }
    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::detect;
    use std::process::Command;

    #[test]
    fn tarball_with_installer_script() {
        let _g = crate::util::TEST_ENV.lock().unwrap_or_else(|e| e.into_inner());
        let d = tempfile::tempdir().unwrap();
        std::env::set_var("XDG_CACHE_HOME", d.path().join("cache"));
        let src = d.path().join("s/app-1");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("install.sh"), "#!/bin/sh\necho hi\n").unwrap();
        let tgz = d.path().join("app-1.tar.gz");
        assert!(Command::new("tar").arg("-czf").arg(&tgz).arg("-C").arg(d.path().join("s")).arg("app-1").status().unwrap().success());
        let plan = analyze(&tgz.to_string_lossy(), &detect(), false, false, &|_, _| true).unwrap();
        assert!(plan.supported, "{:?}", plan.warnings);
        assert_eq!(plan.steps.len(), 2); // extract + run
        assert!(plan.verify.is_some() && plan.sha256.is_some());
        assert!(plan.preview.is_some());
    }

    #[test]
    fn rejects_traversal_archive() {
        let _g = crate::util::TEST_ENV.lock().unwrap_or_else(|e| e.into_inner());
        let d = tempfile::tempdir().unwrap();
        std::env::set_var("XDG_CACHE_HOME", d.path().join("cache"));
        std::fs::write(d.path().join("f"), "x").unwrap();
        let tgz = d.path().join("evil.tar.gz");
        // GNU tar strips leading ../ unless told otherwise; -P keeps it.
        let ok = Command::new("tar").current_dir(d.path().join("cache").parent().unwrap()).args(["-czPf"]).arg(&tgz).args(["--transform", "s,^f$,../evil,"]).arg("-C").arg(d.path()).arg("f").status().unwrap().success();
        if ok {
            assert!(analyze(&tgz.to_string_lossy(), &detect(), false, false, &|_, _| true).is_err());
        }
    }

    #[test]
    fn cancelled_analyze_leaves_no_work_folder() {
        let _g = crate::util::TEST_ENV.lock().unwrap_or_else(|e| e.into_inner());
        let d = tempfile::tempdir().unwrap();
        std::env::set_var("XDG_CACHE_HOME", d.path().join("cache"));
        let f = d.path().join("install.sh");
        std::fs::write(&f, "#!/bin/sh\necho hi\n").unwrap();
        assert_eq!(analyze(&f.to_string_lossy(), &detect(), false, false, &|_, _| false).unwrap_err(), CANCELLED);
        assert_eq!(std::fs::read_dir(cache_dir().join("work")).unwrap().count(), 0);
    }

    #[test]
    fn debian_source_parts() {
        assert_eq!(debian_part("foo_1.2.orig.tar.gz"), Some(("foo_1.2".into(), false)));
        assert_eq!(debian_part("foo_1.2.orig-docs.tar.xz"), Some(("foo_1.2".into(), false)));
        assert_eq!(debian_part("foo_1.2-3.debian.tar.xz"), Some(("foo_1.2".into(), true)));
        assert_eq!(debian_part("foo-1.2.tar.gz"), None);
        let _g = crate::util::TEST_ENV.lock().unwrap_or_else(|e| e.into_inner());
        let d = tempfile::tempdir().unwrap();
        std::env::set_var("XDG_CACHE_HOME", d.path().join("cache"));
        let deb = d.path().join("foo_1.2-3.debian.tar.xz");
        std::fs::write(&deb, "x").unwrap();
        let p = analyze(&deb.to_string_lossy(), &detect(), false, false, &|_, _| true).unwrap();
        assert!(!p.supported && p.advice[0].contains(".dsc"));
        std::fs::write(d.path().join("foo_1.2-3.dsc"), "Source: foo\nVersion: 1.2-3\n").unwrap();
        let p = analyze(&deb.to_string_lossy(), &detect(), false, false, &|_, _| true).unwrap();
        assert_eq!((p.kind.as_str(), p.name.as_str()), ("dsc", "foo"));
    }

    #[test]
    fn missing_file_errors() {
        // analyze makes a work folder in the cache before it fails; other tests count those.
        let _g = crate::util::TEST_ENV.lock().unwrap_or_else(|e| e.into_inner());
        assert!(analyze("/no/such/file.deb", &detect(), false, false, &|_, _| true).is_err());
    }
}
