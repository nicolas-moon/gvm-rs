use std::collections::BTreeMap;
use std::path::Path;

use crate::config::Root;
use crate::version::GoVersion;
use walkdir::WalkDir;

/// Hash of a file: (size, sha256-hex).
type FileMeta = (u64, String);

fn walk_tree(base: &Path) -> anyhow::Result<BTreeMap<String, FileMeta>> {
    use sha2::{Digest, Sha256};
    let mut out = BTreeMap::new();
    for entry in WalkDir::new(base)
        .sort_by_file_name()
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let meta = entry.metadata()?;
        if !meta.is_file() {
            continue;
        }
        let rel = entry.path().strip_prefix(base)?;
        let rel = rel.to_string_lossy().to_string();
        let file = std::fs::File::open(entry.path())?;
        use std::io::Read;
        let mut buf = Vec::with_capacity(64 * 1024);
        {
            let mut f = file;
            let mut chunk = vec![0u8; 64 * 1024];
            loop {
                let n = f.read(&mut chunk)?;
                if n == 0 {
                    break;
                }
                buf.extend_from_slice(&chunk[..n]);
            }
        }
        let mut hasher = Sha256::new();
        hasher.update(&buf);
        let digest = hasher.finalize();
        let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        out.insert(rel, (buf.len() as u64, hex));
    }
    Ok(out)
}

/// Compare the active GOROOT against the clean cached copy.
/// Returns the process exit code (0 = clean, 1 = differences, like diff(1)).
pub fn diff(root: &Root, version_opt: Option<String>) -> anyhow::Result<i32> {
    let version: GoVersion = match version_opt {
        Some(name) => root.resolve_name(&name, &mut Vec::new())?.1,
        None => root.active_version().ok_or_else(|| {
            anyhow::anyhow!(
                "no active Go version (run 'gvm use <version>'), or pass --version <version>"
            )
        })?,
    };
    let goroot = root.gos().join(version.tag());
    let clean = root.cache().join(version.tag());
    if !goroot.is_dir() {
        anyhow::bail!("{version} is not installed");
    }
    if !clean.is_dir() {
        anyhow::bail!("no clean copy of {version} in the cache; reinstall it to take a diff");
    }

    eprintln!(
        "Diffing {} against the clean cached copy…",
        goroot.display()
    );
    let active = walk_tree(&goroot)?;
    let pristine = walk_tree(&clean)?;

    let mut added = Vec::new();
    let mut removed = Vec::new();
    let mut modified = Vec::new();
    for (p, m) in &active {
        match pristine.get(p) {
            Some(pm) if pm == m => {}
            Some(_) => modified.push(p.clone()),
            None => added.push(p.clone()),
        }
    }
    for p in pristine.keys() {
        if !active.contains_key(p) {
            removed.push(p.clone());
        }
    }

    let mut dirty = false;
    if !added.is_empty() {
        println!("Added to GOROOT:");
        for p in &added {
            println!("  A {p}");
        }
        dirty = true;
    }
    if !removed.is_empty() {
        println!("Removed from GOROOT:");
        for p in &removed {
            println!("  R {p}");
        }
        dirty = true;
    }
    if !modified.is_empty() {
        println!("Modified in GOROOT:");
        for p in &modified {
            println!("  M {p}");
        }
        dirty = true;
    }
    if !dirty {
        println!("GOROOT is clean: no differences from the shipped tree.");
        Ok(0)
    } else {
        println!(
            "\n{} file(s) added, {} removed, {} modified.",
            added.len(),
            removed.len(),
            modified.len()
        );
        Ok(1)
    }
}
