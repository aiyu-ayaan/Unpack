use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub const MAX_DOWNLOAD: u64 = 4 * 1024 * 1024 * 1024;

pub struct Downloaded {
    pub path: PathBuf,
    pub sha256: String,
    pub final_url: String,
}

pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(hex(&h.finalize()))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Bytes done and the total when the server sent one; returning false cancels.
pub type Progress<'a> = &'a dyn Fn(u64, Option<u64>) -> bool;

pub const CANCELLED: &str = "Cancelled";

/// Download `url` into `dir`. HTTPS only unless `allow_http`.
pub fn download(url: &str, dir: &Path, allow_http: bool, progress: Progress) -> Result<Downloaded, String> {
    if !allow_http && !url.starts_with("https://") {
        return Err("Refusing plain http:// link. Use the https:// version if the site has one.".into());
    }
    let resp = ureq::AgentBuilder::new()
        .redirects(8)
        .timeout_connect(std::time::Duration::from_secs(20))
        .build()
        .get(url)
        .set("User-Agent", "Unpack/0.1")
        .call()
        .map_err(|e| format!("Download failed: {e}"))?;
    let final_url = resp.get_url().to_string();
    // A redirect target is often an opaque id (GitHub release assets), so prefer the server's file name, then the requested one.
    let name = resp.header("Content-Disposition").and_then(disposition_name).unwrap_or_else(|| {
        let asked = crate::input::url_file_name(url);
        if asked == "download" { crate::input::url_file_name(&final_url) } else { asked }
    });
    let size: Option<u64> = resp.header("Content-Length").and_then(|v| v.parse().ok());
    let path = dir.join(sanitize(&name));
    let mut out = std::fs::File::create(&path).map_err(|e| e.to_string())?;
    let mut reader = resp.into_reader().take(MAX_DOWNLOAD + 1);
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    let mut total = 0u64;
    loop {
        let n = reader.read(&mut buf).map_err(|e| format!("Download failed: {e}"))?;
        if n == 0 {
            break;
        }
        total += n as u64;
        if total > MAX_DOWNLOAD {
            return Err("Download is larger than the 4 GiB safety limit".into());
        }
        hasher.update(&buf[..n]);
        out.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        if !progress(total, size) {
            return Err(CANCELLED.into());
        }
    }
    Ok(Downloaded { path, sha256: hex(&hasher.finalize()), final_url })
}

/// The `filename=` of a Content-Disposition header, reduced to its last path component.
fn disposition_name(h: &str) -> Option<String> {
    let v = h.split(';').find_map(|p| p.trim().strip_prefix("filename="))?;
    let name = v.trim().trim_matches('"').rsplit(['/', '\\']).next()?.to_string();
    if name.is_empty() { None } else { Some(name) }
}

fn sanitize(name: &str) -> String {
    let s: String = name.chars().map(|c| if c == '/' || c == '\0' { '_' } else { c }).collect();
    if s.is_empty() || s == "." || s == ".." { "download".into() } else { s }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_file() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("f");
        std::fs::write(&p, b"abc").unwrap();
        assert_eq!(
            sha256_file(&p).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn rejects_http() {
        let d = tempfile::tempdir().unwrap();
        assert!(download("http://example.com/a", d.path(), false, &|_, _| true).is_err());
    }

    #[test]
    fn sanitizes() {
        assert_eq!(sanitize(".."), "download");
        assert_eq!(sanitize("a/b"), "a_b");
    }

    #[test]
    fn reads_disposition_file_name() {
        assert_eq!(disposition_name("attachment; filename=ripgrep-15.tar.gz").as_deref(), Some("ripgrep-15.tar.gz"));
        assert_eq!(disposition_name("attachment; filename=\"../a b.deb\"").as_deref(), Some("a b.deb"));
        assert_eq!(disposition_name("inline"), None);
    }
}
