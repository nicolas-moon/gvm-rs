use std::process::Command;

use crate::config::Root;
use crate::releases;
use crate::version::GoVersion;

/// `gvm list` — installed versions, newest first, with the active one
/// (per `$GOROOT`) marked `=>`.
pub fn list(root: &Root) -> anyhow::Result<()> {
    let active = root.active_version();
    let installed = root.installed();
    println!("Go:");
    if installed.is_empty() {
        println!("   (none installed)");
        return Ok(());
    }
    let mut seen: Vec<String> = Vec::new();
    for (name, version) in installed {
        if seen.contains(&version.tag()) {
            continue;
        }
        seen.push(version.tag().clone());
        let marker = if Some(&version) == active.as_ref() {
            "=>"
        } else {
            "  "
        };
        let gos_dir = root.gos().join(version.tag());
        let go_bin = gos_dir.join("bin").join("go");
        let extra = if go_bin.is_file() {
            Command::new(&go_bin)
                .arg("version")
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .unwrap_or_default()
        } else {
            String::new()
        };
        let name_part = if name == version.tag() {
            name.clone()
        } else {
            format!("{name} ({})", version.tag())
        };
        if extra.is_empty() {
            println!("{marker} {name_part}");
        } else {
            println!("{marker} {name_part} ({extra})");
        }
    }
    Ok(())
}

/// Compare two version strings for sorting: parseable versions by version,
/// unparseable legacy entries (e.g. the 2012 `go1` release, which predates
/// semver) after everything else. (Do not compare `Result`s directly:
/// `Result::cmp` orders `Err` after `Ok`, which would float garbage to the
/// top of a newest-first display.)
fn version_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    match (GoVersion::parse(a), GoVersion::parse(b)) {
        (Ok(x), Ok(y)) => x.cmp(&y),
        (Ok(_), Err(_)) => std::cmp::Ordering::Greater,
        (Err(_), Ok(_)) => std::cmp::Ordering::Less,
        (Err(_), Err(_)) => a.cmp(b),
    }
}

/// `gvm listall` — available versions from go.dev, newest first.
///
/// The full history is only available with go.dev's `include=all` parameter,
/// so always fetch that; plain `listall` filters to stable releases,
/// `listall --all` shows everything (beta/RC included).
pub fn listall(all: bool) -> anyhow::Result<()> {
    eprintln!("Fetching version list from go.dev…");
    let rels = releases::fetch(true)?;
    let mut sorted: Vec<&releases::Release> = rels.iter().collect();
    if !all {
        sorted.retain(|r| r.stable);
    }
    sorted.sort_by(|a, b| version_cmp(&a.version, &b.version));
    println!("Go:");
    for r in sorted.iter().rev() {
        println!("   {}", r.version);
    }
    println!();
    if all {
        println!("Includes beta/RC versions.");
    }
    println!("Install one with: gvm install <version>");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_cmp_puts_legacy_last() {
        // Regression: go.dev's JSON contains legacy entries like "go1"
        // (pre-semver) that do not parse. They must sort after all real
        // versions, and newest-first display must lead with the newest
        // release — not with an Err-keyed entry.
        let mut vers = vec![
            "go1",
            "go1.27.4",
            "go1.27.3",
            "go1.19",
            "go1.3",
            "go1.19.13",
            "go1.2.2",
        ];
        vers.sort_by(|a, b| version_cmp(a, b));
        assert_eq!(
            vers,
            vec![
                "go1",
                "go1.2.2",
                "go1.3",
                "go1.19",
                "go1.19.13",
                "go1.27.3",
                "go1.27.4"
            ]
        );
    }
}
