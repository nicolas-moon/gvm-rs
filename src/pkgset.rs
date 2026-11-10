use std::fs;

use crate::config::{validate_name, Root};
use crate::util;
use crate::version::GoVersion;

/// The version active in this shell (from `$GOROOT`), when it is a
/// gvm-managed GOROOT.
pub fn current_version(root: &Root) -> anyhow::Result<GoVersion> {
    root.active_version().ok_or_else(|| {
        anyhow::anyhow!("no gvm Go version is active in this shell; run 'gvm use <version>' first")
    })
}

/// `export GOPATH='...'` lines for the shim to eval.
pub fn gopath_exports(dir: &std::path::Path) -> String {
    format!(
        "export GOPATH='{}'\n",
        util::shell_single_quote(dir.to_str().unwrap_or_default())
    )
}

pub fn create(root: &Root, name: &str) -> anyhow::Result<String> {
    let version = current_version(root)?;
    validate_name(name)?;
    if name == "default" {
        anyhow::bail!("the 'default' pkgset always exists; use another name");
    }
    let dir = root.packages().join(version.tag()).join(name);
    fs::create_dir_all(&dir)?;
    eprintln!(
        "Created pkgset {name} for {} and now using it",
        version.tag()
    );
    Ok(gopath_exports(&dir))
}

pub fn use_pkgset(root: &Root, name: &str) -> anyhow::Result<String> {
    let version = current_version(root)?;
    validate_name(name)?;
    let dir = root.packages().join(version.tag()).join(name);
    if !dir.exists() {
        if name == "default" {
            fs::create_dir_all(&dir)?;
        } else {
            let existing: Vec<String> = fs::read_dir(root.packages().join(version.tag()))
                .ok()
                .into_iter()
                .flatten()
                .flatten()
                .filter_map(|e| e.file_name().to_str().map(String::from))
                .collect();
            anyhow::bail!(
                "pkgset {name} does not exist for {} (existing: {})",
                version.tag(),
                if existing.is_empty() {
                    "(none)".to_string()
                } else {
                    existing.join(", ")
                }
            );
        }
    }
    eprintln!("Now using pkgset {name} for {}", version.tag());
    Ok(gopath_exports(&dir))
}

pub fn delete(root: &Root, name: &str, yes: bool) -> anyhow::Result<()> {
    let version = current_version(root)?;
    validate_name(name)?;
    if name == "default" {
        anyhow::bail!("cannot delete the 'default' pkgset");
    }
    let dir = root.packages().join(version.tag()).join(name);
    if !dir.exists() {
        anyhow::bail!("pkgset {name} does not exist for {}", version.tag());
    }
    if !yes
        && !util::confirm(&format!(
            "Delete pkgset {name} for {} (and everything in it)?",
            version.tag()
        ))?
    {
        println!("Aborted.");
        return Ok(());
    }
    fs::remove_dir_all(&dir)?;
    if let Ok(gopath) = std::env::var("GOPATH") {
        if dir == std::path::Path::new(&gopath) {
            eprintln!("Note: GOPATH still points at the deleted pkgset; run 'gvm use' to fix it.");
        }
    }
    eprintln!("Deleted pkgset {name} for {}", version.tag());
    Ok(())
}

pub fn current(_root: &Root) -> anyhow::Result<String> {
    let Ok(gopath) = std::env::var("GOPATH") else {
        return Ok("none".to_string());
    };
    let p = std::path::PathBuf::from(gopath);
    let file = p.file_name().and_then(|s| s.to_str());
    let parent = p
        .parent()
        .and_then(|s| s.file_name().and_then(|s| s.to_str()));
    let grand = p
        .parent()
        .and_then(|s| s.parent().and_then(|s| s.file_name()));
    if let (Some(name), Some(ver), Some(pkgs)) = (file, parent, grand.map(|g| g.to_str())) {
        if pkgs == Some("packages") && ver != "gos" {
            let v = GoVersion::parse(ver);
            if v.is_ok() {
                return Ok(name.to_string());
            }
        }
    }
    Ok("none".to_string())
}

pub fn list(root: &Root) -> anyhow::Result<()> {
    let version = current_version(root)?;
    let base = root.packages().join(version.tag());
    let Ok(entries) = fs::read_dir(&base) else {
        println!("(no pkgsets for {})", version.tag());
        return Ok(());
    };
    let mut names: Vec<String> = entries
        .flatten()
        .filter_map(|e| e.file_name().to_str().map(String::from))
        .collect();
    names.sort();
    let active: Option<std::path::PathBuf> = std::env::var("GOPATH")
        .ok()
        .as_deref()
        .map(std::path::PathBuf::from);
    println!("Pkgsets for {}:", version.tag());
    if names.is_empty() {
        println!("   (none)");
    }
    for name in names {
        let marker = if active == Some(base.join(&name)) {
            "=>"
        } else {
            "  "
        };
        println!("{marker} {name}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::version::GoVersion;
    use std::sync::MutexGuard;

    /// Tests mutate process env vars (GOROOT/GOPATH), so they must not run in
    /// parallel with each other.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Set up a root with go1.21.0 "installed" and make GOROOT point at it.
    /// Returns a guard serializing env-mutating tests for the test's lifetime.
    fn setup(tag: &str) -> (tempfile::TempDir, Root, MutexGuard<'static, ()>) {
        let guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().unwrap();
        let root = Root::new(dir.path().join(".gvm"));
        root.ensure_dirs().unwrap();
        let v = GoVersion::parse(tag).unwrap();
        std::fs::create_dir_all(root.gos().join(v.tag()).join("bin")).unwrap();
        std::fs::create_dir_all(root.packages().join(v.tag()).join("default")).unwrap();
        root.install_name(tag, &v).unwrap();
        std::env::set_var("GOROOT", root.gos().join(v.tag()));
        (dir, root, guard)
    }

    #[test]
    fn create_use_delete() {
        let (_dir, root, _guard) = setup("go1.21.0");
        std::env::set_var("GOPATH", "");
        let out = create(&root, "work").unwrap();
        assert!(out.contains("export GOPATH="));
        assert!(root.packages().join("go1.21.0").join("work").is_dir());

        let out = use_pkgset(&root, "work").unwrap();
        assert!(out.contains("export GOPATH="));

        delete(&root, "work", true).unwrap();
        assert!(!root.packages().join("go1.21.0").join("work").exists());
    }

    #[test]
    fn refuse_default_delete_and_create() {
        let (_dir, root, _guard) = setup("go1.21.0");
        assert!(delete(&root, "default", true).is_err());
        assert!(create(&root, "default").is_err());
    }

    #[test]
    fn use_missing_pkgset_fails() {
        let (_dir, root, _guard) = setup("go1.21.0");
        assert!(use_pkgset(&root, "nope",).is_err());
    }

    #[test]
    fn current_reports_name() {
        let (_dir, root, _guard) = setup("go1.21.0");
        let p = root.packages().join("go1.21.0").join("work");
        std::fs::create_dir_all(&p).unwrap();
        std::env::set_var("GOPATH", p);
        assert_eq!(current(&root).unwrap(), "work");
    }
}
