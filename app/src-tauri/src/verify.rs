//! Check a reviewed file against what its publisher printed: a pasted SHA-256, a `SHA256SUMS` file, or a detached
//! OpenPGP signature (`.sig`, `.asc`, `.gpg`) checked with `gpg` against the user's own keyring.
use crate::fetch::{self, Progress};
use crate::plan::Plan;
use crate::util::{command, has};
use std::path::{Path, PathBuf};

#[derive(Debug, PartialEq)]
pub enum Verdict {
    Match(String),
    Mismatch(String),
    /// Could not tell either way.
    Unknown(String),
}

fn is_hash(s: &str) -> bool {
    s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit())
}

/// A SHA-256 the user pasted: bare, `sha256:<hex>`, or a `<hex>  file` line.
pub fn pasted_hash(text: &str) -> Option<String> {
    let t = text.trim();
    let t = t.strip_prefix("sha256:").or_else(|| t.strip_prefix("SHA256:")).unwrap_or(t);
    t.split_whitespace().next().filter(|w| is_hash(w)).map(str::to_lowercase)
}

/// Compare with a `SHA256SUMS`-style list (`<hex>  name`, also `SHA256 (name) = <hex>`).
pub fn sums_verdict(list: &str, file: &str, sha: &str) -> Verdict {
    let mut named = None;
    for l in list.lines() {
        let words: Vec<&str> = l.split_whitespace().collect();
        let hash = words.iter().find(|w| is_hash(w)).map(|w| w.to_lowercase());
        if hash.as_deref() == Some(sha) {
            return Verdict::Match("The file's SHA-256 is in the published checksum list.".into());
        }
        if hash.is_some() && words.iter().any(|w| w.trim_start_matches('*').trim_matches(['(', ')']) == file) {
            named = hash;
        }
    }
    match named {
        Some(_) => Verdict::Mismatch(format!("The checksum list has a different SHA-256 for {file}. The file is not the one that was published.")),
        None => Verdict::Unknown(format!("The checksum list does not mention {file} or its SHA-256.")),
    }
}

/// What `gpg --status-fd 1 --verify` reported.
pub fn gpg_verdict(status: &str) -> Verdict {
    let line = |tag: &str| status.lines().find_map(|l| l.strip_prefix(&format!("[GNUPG:] {tag} ")).map(str::to_string));
    if let Some(who) = line("GOODSIG") {
        let signer = who.split_once(' ').map_or(who.as_str(), |x| x.1).to_string();
        let trusted = status.contains("[GNUPG:] TRUST_FULLY") || status.contains("[GNUPG:] TRUST_ULTIMATE");
        let note = if trusted { "" } else { " Make sure that key really belongs to the project (its website lists it)." };
        return Verdict::Match(format!("Good signature from {signer}.{note}"));
    }
    if line("BADSIG").is_some() {
        return Verdict::Mismatch("The signature does not match this file. It was changed after it was signed.".into());
    }
    if let Some(key) = line("NO_PUBKEY") {
        return Verdict::Unknown(format!("The signature could not be checked: the signer's key ({key}) is not on this computer. Import the project's key (from its website, or `gpg --recv-keys {key}`) and check again."));
    }
    Verdict::Unknown("gpg could not read that signature.".into())
}

fn gpg(sig: &Path, file: &Path) -> Result<Verdict, String> {
    if !has("gpg") {
        return Err("Checking a signature needs gpg (the gnupg package).".into());
    }
    let out = command("gpg").args(["--batch", "--status-fd", "1", "--verify"]).arg(sig).arg(file).env("LC_ALL", "C").output().map_err(|e| e.to_string())?;
    Ok(gpg_verdict(&String::from_utf8_lossy(&out.stdout)))
}

/// Check `plan`'s file against `expected` and record the answer on the plan: advice when it matches, a refusal
/// when it does not, a warning when it could not be told. `work` holds a downloaded signature or list.
pub fn check(plan: &mut Plan, expected: &str, work: &Path, progress: Progress) -> Result<(), String> {
    let Some((file, sha)) = plan.verify.clone() else {
        return Err("Only a downloaded or local file can be checked; this plan has none.".into());
    };
    let e = expected.trim();
    let verdict = if let Some(h) = pasted_hash(e) {
        if h == sha {
            Verdict::Match("The file's SHA-256 matches the one you pasted.".into())
        } else {
            Verdict::Mismatch("The file's SHA-256 does not match the one you pasted. The file is not the one that was published.".into())
        }
    } else {
        let local: PathBuf = if e.starts_with("https://") {
            std::fs::create_dir_all(work).map_err(|x| x.to_string())?;
            fetch::download(e, work, false, progress)?.path
        } else {
            let p = PathBuf::from(e.strip_prefix("~/").map(|r| crate::util::home().join(r).to_string_lossy().into_owned()).unwrap_or_else(|| e.into()));
            if !p.is_file() {
                return Err("Paste a SHA-256, or an https:// link or file path of a signature (.sig, .asc) or checksum list.".into());
            }
            p
        };
        let head = std::fs::read(&local).map_err(|x| x.to_string())?;
        let text = String::from_utf8_lossy(&head[..head.len().min(1 << 20)]).into_owned();
        // A checksum list is text with hashes in it; anything else is taken as a signature.
        if !text.contains("BEGIN PGP SIGNATURE") && text.split_whitespace().any(is_hash) {
            sums_verdict(&text, &file.file_name().unwrap_or_default().to_string_lossy(), &sha)
        } else {
            gpg(&local, &file)?
        }
    };
    match verdict {
        Verdict::Match(m) => plan.advice.insert(0, m),
        Verdict::Mismatch(m) => plan.refuse(&m, &["Do not install it. Download it again from the project's own site."]),
        Verdict::Unknown(m) => plan.warnings.push(m),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const H: &str = "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08";

    #[test]
    fn hashes_and_lists() {
        assert_eq!(pasted_hash(&format!(" sha256:{} ", H.to_uppercase())).as_deref(), Some(H));
        assert_eq!(pasted_hash(&format!("{H}  app.deb")).as_deref(), Some(H));
        assert_eq!(pasted_hash("https://x/app.sig"), None);
        let other = "0".repeat(64);
        assert!(matches!(sums_verdict(&format!("{other}  a.deb\n{H} *app.deb\n"), "app.deb", H), Verdict::Match(_)));
        assert!(matches!(sums_verdict(&format!("SHA256 (app.deb) = {other}\n"), "app.deb", H), Verdict::Mismatch(_)));
        assert!(matches!(sums_verdict(&format!("{other}  b.deb\n"), "app.deb", H), Verdict::Unknown(_)));
    }

    #[test]
    fn reads_gpg_status() {
        assert_eq!(gpg_verdict("[GNUPG:] NEWSIG\n[GNUPG:] GOODSIG ABCD Jane Dev <j@x>\n[GNUPG:] TRUST_ULTIMATE 0 pgp\n"), Verdict::Match("Good signature from Jane Dev <j@x>.".into()));
        assert!(matches!(gpg_verdict("[GNUPG:] GOODSIG ABCD Jane\n[GNUPG:] TRUST_UNDEFINED\n"), Verdict::Match(m) if m.contains("really belongs")));
        assert!(matches!(gpg_verdict("[GNUPG:] BADSIG ABCD Jane\n"), Verdict::Mismatch(_)));
        assert!(matches!(gpg_verdict("[GNUPG:] NO_PUBKEY 1234ABCD\n"), Verdict::Unknown(m) if m.contains("1234ABCD")));
    }

    #[test]
    fn marks_the_plan() {
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join("app.deb");
        std::fs::write(&f, "test").unwrap();
        let mut p = Plan::new("deb", "app", "x");
        p.verify = Some((f.clone(), H.into()));
        check(&mut p, H, d.path(), &|_, _| true).unwrap();
        assert!(p.supported && p.advice[0].contains("matches"));
        check(&mut p, &"1".repeat(64), d.path(), &|_, _| true).unwrap();
        assert!(!p.supported);
        let list = d.path().join("SHA256SUMS");
        std::fs::write(&list, format!("{H}  app.deb\n")).unwrap();
        let mut q = Plan::new("deb", "app", "x");
        q.verify = Some((f, H.into()));
        check(&mut q, &list.to_string_lossy(), d.path(), &|_, _| true).unwrap();
        assert!(q.supported && q.advice[0].contains("list"));
        assert!(check(&mut Plan::new("git", "x", "y"), H, d.path(), &|_, _| true).is_err());
    }
}
