//! Version metadata from `https://go.dev/dl/?mode=json`.

use serde::Deserialize;

use crate::version::GoVersion;

#[derive(Debug, Clone, Deserialize)]
pub struct Release {
    pub version: String,
    pub stable: bool,
    #[serde(default)]
    pub files: Vec<ReleaseFile>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReleaseFile {
    pub filename: String,
    pub os: String,
    pub arch: String,
    #[serde(default)]
    pub sha256: String,
    #[serde(default)]
    pub kind: String,
}

/// Fetch release metadata.
///
/// `include_all` mirrors the go.dev `include=all` parameter. Note that as of
/// 2025 the default endpoint only returns the two most recent release lines,
/// so callers that need the full history (listall, resolving partial versions
/// like `1.20`, …) must pass `include_all = true`.
pub fn fetch(include_all: bool) -> anyhow::Result<Vec<Release>> {
    let url = if include_all {
        "https://go.dev/dl/?mode=json&include=all"
    } else {
        "https://go.dev/dl/?mode=json"
    };
    let body = reqwest::blocking::get(url)?.error_for_status()?.text()?;
    Ok(serde_json::from_str(&body)?)
}

/// Parse a release's version, mapping unparseable legacy entries (e.g. the
/// 2012 `go1`) to a minimal version so they sort before everything else.
fn version_of(r: &Release) -> GoVersion {
    GoVersion::parse(&r.version).unwrap_or_else(|_| GoVersion::Release {
        major: 0,
        minor: 0,
        patch: None,
        prerelease: Some("unknown".into()),
    })
}

/// Newest stable release in the list.
pub fn latest_stable(rels: &[Release]) -> Option<&Release> {
    rels.iter()
        .filter(|r| r.stable)
        .max_by(|a, b| version_of(a).cmp(&version_of(b)))
}

/// Newest stable release in the `major.minor` line (for resolving partial
/// inputs like `1.21`).
pub fn resolve_prefix(rels: &[Release], major: u32, minor: u32) -> Option<Release> {
    rels.iter()
        .filter(|r| r.stable)
        .filter(|r| {
            matches!(&version_of(r), GoVersion::Release {
                major: m,
                minor: n,
                patch: Some(_),
                prerelease: None,
            } if *m == major && *n == minor)
        })
        .max_by(|a, b| version_of(a).cmp(&version_of(b)))
        .cloned()
}

/// The prebuilt binary tarball file for a release, os and arch.
///
/// go.dev metadata has used `kind: "binary"` historically and
/// `kind: "archive"` in newer payloads, so accept both.
pub fn find_binary_file(rel: &Release, os: &str, arch: &str) -> Option<ReleaseFile> {
    rel.files
        .iter()
        .find(|f| f.os == os && f.arch == arch && (f.kind == "binary" || f.kind == "archive"))
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    const JSON: &str = r#"[
        {
            "version": "go1.20.14",
            "stable": true,
            "files": [
                {"filename": "go1.20.14.linux-amd64.tar.gz", "os": "linux", "arch": "amd64", "sha256": "abc", "kind": "binary"},
                {"filename": "go1.20.14.tar.gz", "os": "linux", "arch": "amd64", "sha256": "def", "kind": "source"}
            ]
        },
        {
            "version": "go1.21.5",
            "stable": true,
            "files": [
                {"filename": "go1.21.5.linux-amd64.tar.gz", "os": "linux", "arch": "amd64", "sha256": "111", "kind": "binary"}
            ]
        },
        {
            "version": "go1.22rc1",
            "stable": false,
            "files": []
        },
        {
            "version": "go1.23.0",
            "stable": true,
            "files": [
                {"filename": "go1.23.0.linux-amd64.tar.gz", "os": "linux", "arch": "amd64", "sha256": "222", "kind": "archive"}
            ]
        }
    ]"#;

    #[test]
    fn latest_stable_picks_highest() {
        let rels: Vec<Release> = serde_json::from_str(JSON).unwrap();
        let r = latest_stable(&rels).unwrap();
        assert_eq!(r.version, "go1.23.0");
    }

    #[test]
    fn resolve_prefix_finds_line() {
        let rels: Vec<Release> = serde_json::from_str(JSON).unwrap();
        let r = resolve_prefix(&rels, 1, 20).unwrap();
        assert_eq!(r.version, "go1.20.14");
        assert!(resolve_prefix(&rels, 1, 99).is_none());
    }

    #[test]
    fn find_binary_file_by_os_arch() {
        let rels: Vec<Release> = serde_json::from_str(JSON).unwrap();
        let r = rels.iter().find(|r| r.version == "go1.21.5").unwrap();
        let f = find_binary_file(r, "linux", "amd64").unwrap();
        assert_eq!(f.filename, "go1.21.5.linux-amd64.tar.gz");
        assert_eq!(f.sha256, "111");
        assert!(find_binary_file(r, "windows", "amd64").is_none());
    }

    #[test]
    fn find_binary_file_accepts_archive_kind() {
        // Newer go.dev payloads use kind: "archive" instead of "binary".
        let rels: Vec<Release> = serde_json::from_str(JSON).unwrap();
        let r = rels.iter().find(|r| r.version == "go1.23.0").unwrap();
        let f = find_binary_file(r, "linux", "amd64").unwrap();
        assert_eq!(f.filename, "go1.23.0.linux-amd64.tar.gz");
        assert_eq!(f.sha256, "222");
    }
}
