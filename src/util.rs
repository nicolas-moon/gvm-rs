use std::fs;
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use crate::config::Root;
use crate::version::GoVersion;

pub fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("/root"))
}

/// Download `url` to `dest` (blocking), reporting progress on stderr.
pub fn download(url: &str, dest: &Path) -> anyhow::Result<()> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    eprintln!("Downloading {url}");
    let resp = reqwest::blocking::get(url)?;
    let status = resp.status();
    if !status.is_success() {
        anyhow::bail!("download failed: HTTP {status} for {url}");
    }
    let mut reader = resp;
    let mut file = fs::File::create(dest)?;
    let written = io::copy(&mut reader, &mut file)?;
    file.sync_all()?;
    eprintln!(
        "Downloaded {} ({} bytes)",
        human_size(written as u64),
        written
    );
    Ok(())
}

/// sha256 of a file, as lowercase hex.
pub fn sha256_file(path: &Path) -> anyhow::Result<String> {
    use sha2::{Digest, Sha256};
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    io::copy(&mut file, &mut hasher)?;
    let digest = hasher.finalize();
    Ok(digest.iter().map(|b| format!("{b:02x}")).collect())
}

/// Extract a `.tar.gz` into `dest`. With `strip_top`, the single top-level
/// directory of the archive (Go tarballs always have one, e.g. `go1.21.0/`
/// or `go/`) is dropped so `dest` directly contains the tree's contents.
pub fn extract_targz(archive: &Path, dest: &Path, strip_top: bool) -> anyhow::Result<()> {
    let file = fs::File::open(archive)?;
    let decoder = flate2::read::GzDecoder::new(file);
    let mut ar = tar::Archive::new(decoder);
    fs::create_dir_all(dest)?;
    let mut top: Option<String> = None;
    for entry in ar.entries()? {
        let mut entry = entry?;
        let original = entry.path()?;
        let mut comps: Vec<Component<'_>> = original.components().collect();
        if comps.iter().any(|c| {
            matches!(
                c,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        }) {
            anyhow::bail!("unsafe path in archive: {}", original.display());
        }
        if comps.is_empty() {
            continue;
        }
        if strip_top {
            let t = comps
                .first()
                .map(|c| c.as_os_str())
                .map(|s| s.to_string_lossy().to_string());
            match &top {
                None => top = t,
                Some(prev) => {
                    if t.as_deref() != Some(prev.as_str()) {
                        anyhow::bail!("archive has multiple top-level entries; cannot strip");
                    }
                }
            }
            comps.remove(0);
        }
        if comps.is_empty() {
            continue;
        }
        let relative: PathBuf = comps.iter().collect();
        let dest_path = dest.join(relative);
        if let Some(parent) = dest_path.parent() {
            fs::create_dir_all(parent)?;
        }
        entry.unpack(&dest_path)?;
    }
    Ok(())
}

/// Ask for a y/N confirmation on stdin.
pub fn confirm(prompt: &str) -> anyhow::Result<bool> {
    print!("{prompt} [y/N] ");
    io::stdout().flush()?;
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    Ok(matches!(
        line.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}

pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["KB", "MB", "GB", "TB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    // Start in KB so unit indexes line up with UNITS.
    let mut size = bytes as f64 / 1024.0;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    format!("{size:.1} {}", UNITS[unit])
}

/// Escape a string for inclusion inside single quotes in shell code.
pub fn shell_single_quote(s: &str) -> String {
    s.replace('\'', "'\\''")
}

/// Find a usable "bootstrap" Go toolchain for compiling a new one from
/// source: prefer the newest gvm-installed version, then `go` on `$PATH`.
///
/// (Go 1.5+ ships a Go-written toolchain, so bootstrapping a source build
/// needs some working Go first — same constraint as the original gvm.)
pub fn find_bootstrap(root: &Root, target: &GoVersion) -> anyhow::Result<PathBuf> {
    for (_, v) in root.installed() {
        if v == *target {
            continue;
        }
        let bin = root.gos().join(v.tag()).join("bin").join("go");
        if bin.is_file() {
            return Ok(bin);
        }
    }
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            let candidate = dir.join("go");
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }
    anyhow::bail!(
        "no bootstrap Go toolchain found to compile from source.\n\
         Install a prebuilt version first, e.g.:\n\
           gvm-rs install go1.21 --binary\n\
           gvm use go1.21\n\
         then retry the source install."
    )
}

/// Put `dir` first on `$PATH` and return the joined value.
pub fn prepend_path(dir: &Path) -> anyhow::Result<std::ffi::OsString> {
    let mut paths: Vec<std::ffi::OsString> = vec![std::ffi::OsString::from(dir)];
    if let Some(p) = std::env::var_os("PATH") {
        for path in std::env::split_paths(&p) {
            paths.push(path.into_os_string());
        }
    }
    std::env::join_paths(paths).map_err(|e| anyhow::anyhow!("could not build PATH: {e}"))
}

/// `uname -s -m` as (sysname, machine).
pub fn uname() -> anyhow::Result<(String, String)> {
    let out = Command::new("uname").args(["-s", "-m"]).output()?;
    if !out.status.success() {
        anyhow::bail!("uname failed");
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let mut it = s.split_whitespace();
    match (it.next(), it.next()) {
        (Some(a), Some(b)) => Ok((a.to_string(), b.to_string())),
        _ => anyhow::bail!("could not parse uname output: {s:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_size_units() {
        assert_eq!(human_size(512), "512 B");
        assert_eq!(human_size(1024), "1.0 KB");
        assert_eq!(human_size(1_048_576), "1.0 MB");
        assert_eq!(human_size(1_073_741_824), "1.0 GB");
        // The regression: 70680823 bytes is ~67 MB, not GB.
        assert_eq!(human_size(70680823), "67.4 MB");
    }
}
