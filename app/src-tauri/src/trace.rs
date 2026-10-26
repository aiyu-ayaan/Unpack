use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Roots watched for files created by installers that no package manager tracks.
pub fn roots() -> Vec<PathBuf> {
    vec![crate::util::home().join(".local"), PathBuf::from("/opt"), PathBuf::from("/usr/local")]
}

pub fn snapshot(roots: &[PathBuf]) -> BTreeSet<String> {
    let skip = [crate::util::data_dir(), crate::util::cache_dir()];
    let mut set = BTreeSet::new();
    for r in roots {
        let walker = walkdir::WalkDir::new(r).follow_links(false).into_iter().filter_entry(|e| !skip.iter().any(|s| e.path().starts_with(s)));
        for e in walker.flatten() {
            if !e.file_type().is_dir() {
                set.insert(e.path().to_string_lossy().into_owned());
            }
        }
    }
    set
}

/// Files present in `after` but not in `before`.
pub fn diff(before: &BTreeSet<String>, after: &BTreeSet<String>) -> Vec<String> {
    after.difference(before).cloned().collect()
}

/// Remove now-empty parent dirs of removed files, stopping at a watched root.
pub fn prune_empty_parents(path: &Path, roots: &[PathBuf]) {
    let mut cur = path.parent();
    while let Some(d) = cur {
        if roots.iter().any(|r| r == d) || std::fs::remove_dir(d).is_err() {
            break;
        }
        cur = d.parent();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_new_files() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("old"), "1").unwrap();
        let roots = vec![d.path().to_path_buf()];
        let before = snapshot(&roots);
        std::fs::create_dir(d.path().join("sub")).unwrap();
        std::fs::write(d.path().join("sub/new"), "2").unwrap();
        let after = snapshot(&roots);
        let new = diff(&before, &after);
        assert_eq!(new.len(), 1);
        assert!(new[0].ends_with("sub/new"));
    }

    #[test]
    fn prunes_up_to_root() {
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join("a/b/f");
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(&f, "x").unwrap();
        std::fs::remove_file(&f).unwrap();
        prune_empty_parents(&f, &[d.path().to_path_buf()]);
        assert!(!d.path().join("a").exists());
        assert!(d.path().exists());
    }
}
