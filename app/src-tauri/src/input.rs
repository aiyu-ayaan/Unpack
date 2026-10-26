use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
pub enum Source {
    File(PathBuf),
    Url { url: String, pipe_to_shell: bool },
    Git { url: String },
}

const GIT_HOSTS: [&str; 4] = ["github.com", "gitlab.com", "codeberg.org", "bitbucket.org"];

fn extract_url(s: &str) -> Option<String> {
    let i = s.find("https://").or_else(|| s.find("http://"))?;
    let url: String = s[i..]
        .chars()
        .take_while(|c| !c.is_whitespace() && !"\"'`)|;<>".contains(*c))
        .collect();
    Some(url)
}

/// True when the command feeds downloaded text to an interpreter.
fn pipes_to_shell(cmd: &str) -> bool {
    let shells = ["sh", "bash", "zsh", "dash", "ash"];
    let piped = cmd.split('|').skip(1).any(|seg| {
        let mut words = seg.split_whitespace();
        let first = words.next().unwrap_or("");
        let cmd = if first == "sudo" { words.next().unwrap_or("") } else { first };
        shells.contains(&cmd.rsplit('/').next().unwrap_or(""))
    });
    piped || cmd.contains("sh -c") || cmd.contains("<(")
}

fn is_git_url(url: &str) -> bool {
    if url.starts_with("git@") || url.starts_with("git://") || url.starts_with("ssh://") {
        return true;
    }
    let Some(rest) = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://")) else {
        return false;
    };
    let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
    let path = path.trim_end_matches('/');
    if path.ends_with(".git") {
        return true;
    }
    // owner/repo on a known forge, nothing deeper.
    GIT_HOSTS.contains(&host) && path.split('/').filter(|s| !s.is_empty()).count() == 2
}

pub fn parse(input: &str) -> Result<Source, String> {
    let input = input.trim();
    if input.is_empty() {
        return Err("Nothing to install: input is empty".into());
    }
    if let Some(rest) = input.strip_prefix("git clone ") {
        let url = rest.split_whitespace().find(|w| w.contains("://") || w.starts_with("git@")).unwrap_or("");
        return Ok(Source::Git { url: url.to_string() });
    }
    if input.starts_with("git@") || input.starts_with("git://") || input.starts_with("ssh://") {
        return Ok(Source::Git { url: input.to_string() });
    }
    let is_cmd = input.contains(char::is_whitespace);
    if is_cmd || input.starts_with("http://") || input.starts_with("https://") {
        let url = extract_url(input).ok_or("Could not find a http(s) link in that command")?;
        if !is_cmd && is_git_url(&url) {
            return Ok(Source::Git { url });
        }
        return Ok(Source::Url { pipe_to_shell: is_cmd && pipes_to_shell(input), url });
    }
    let path = if let Some(p) = input.strip_prefix("file://") {
        PathBuf::from(p)
    } else if let Some(p) = input.strip_prefix("~/") {
        crate::util::home().join(p)
    } else {
        PathBuf::from(input)
    };
    if path.exists() {
        Ok(Source::File(path))
    } else {
        Err(format!("File not found: {}", path.display()))
    }
}

/// File name guess from a URL path, without query string.
pub fn url_file_name(url: &str) -> String {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    let name = path.rsplit('/').next().unwrap_or("");
    if name.is_empty() { "download".into() } else { name.to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(s: &str) -> (String, bool) {
        match parse(s).unwrap() {
            Source::Url { url, pipe_to_shell } => (url, pipe_to_shell),
            other => panic!("not url: {other:?}"),
        }
    }

    #[test]
    fn curl_pipe() {
        assert_eq!(url("curl -fsSL https://x.io/install.sh | sh"), ("https://x.io/install.sh".into(), true));
        assert_eq!(url("curl -sSL https://x.io/i.sh | sudo bash"), ("https://x.io/i.sh".into(), true));
        assert_eq!(url("wget -qO- https://x.io/i.sh | /bin/bash"), ("https://x.io/i.sh".into(), true));
        assert_eq!(url("bash -c \"$(curl -fsSL https://x.io/i.sh)\""), ("https://x.io/i.sh".into(), true));
    }

    #[test]
    fn plain_download() {
        assert_eq!(url("wget https://x.io/app.AppImage"), ("https://x.io/app.AppImage".into(), false));
        assert_eq!(url("curl -LO https://x.io/a.deb"), ("https://x.io/a.deb".into(), false));
        assert_eq!(url("https://x.io/a.tar.gz"), ("https://x.io/a.tar.gz".into(), false));
    }

    #[test]
    fn git() {
        for s in [
            "https://github.com/owner/repo",
            "https://github.com/owner/repo.git",
            "git@github.com:owner/repo.git",
            "https://example.org/x/y.git",
        ] {
            assert!(matches!(parse(s).unwrap(), Source::Git { .. }), "{s}");
        }
        assert_eq!(parse("git clone --depth 1 https://h.io/a.git").unwrap(), Source::Git { url: "https://h.io/a.git".into() });
        // release asset on GitHub is a download, not a repo
        assert!(matches!(parse("https://github.com/o/r/releases/download/v1/a.deb").unwrap(), Source::Url { .. }));
    }

    #[test]
    fn errors() {
        assert!(parse("").is_err());
        assert!(parse("/definitely/not/here").is_err());
        assert!(parse("curl nothing").is_err());
    }

    #[test]
    fn names() {
        assert_eq!(url_file_name("https://x.io/a/b.deb?token=1"), "b.deb");
        assert_eq!(url_file_name("https://x.io/"), "download");
    }
}
