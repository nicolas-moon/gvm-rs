use crate::config::Root;
use crate::util;

/// Activate a version in the current shell.
///
/// Returns shell code (stdout of the shim's `eval`) and prints a human-facing
/// note on stderr.
pub fn use_version(root: &Root, name: Option<&str>, set_default: bool) -> anyhow::Result<String> {
    let name = match name {
        Some(n) => n.to_string(),
        None => match root.default_name() {
            Some(d) => d,
            None => anyhow::bail!(
                "no version given and no default set.\n\
                 Activate one with:  gvm use <version> --default"
            ),
        },
    };
    let (resolved_name, version) = root.resolve_name(&name, &mut Vec::new())?;
    let gos_dir = root.gos().join(version.tag());
    if !gos_dir.is_dir() {
        anyhow::bail!(
            "{version} is not installed.\n\
             Install it with:  gvm install {version}\n\
             See available versions with:  gvm listall"
        );
    }
    if set_default {
        root.set_default(&resolved_name)?;
    }
    // Ensure the default pkgset exists.
    let gopath = root.packages().join(version.tag()).join("default");
    std::fs::create_dir_all(&gopath)?;
    let overlay = root.overlays().join(version.tag());
    let bin = gos_dir.join("bin");

    eprintln!(
        "Now using version {version} (goroot: {}{})",
        gos_dir.display(),
        if set_default { ", set as default" } else { "" }
    );
    let exports = format!(
        "unset GOROOT GOPATH GVM_OVERLAY_PREFIX\n\
         export GOROOT='{}'\n\
         export GOPATH='{}'\n\
         export GVM_OVERLAY_PREFIX='{}'\n\
         export PATH='{}:${{PATH}}'\n",
        util::shell_single_quote(gos_dir.to_str().unwrap_or_default()),
        util::shell_single_quote(gopath.to_str().unwrap_or_default()),
        util::shell_single_quote(overlay.to_str().unwrap_or_default()),
        util::shell_single_quote(bin.to_str().unwrap_or_default()),
    );
    Ok(exports)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::version::GoVersion;

    #[test]
    fn use_emits_env_lines() {
        let dir = tempfile::tempdir().unwrap();
        let root = Root::new(dir.path().join(".gvm"));
        root.ensure_dirs().unwrap();
        let v = GoVersion::parse("go1.21.0").unwrap();
        std::fs::create_dir_all(root.gos().join(v.tag()).join("bin")).unwrap();
        root.install_name("go1.21.0", &v).unwrap();
        root.set_default("go1.21.0").unwrap();

        let out = use_version(&root, None, false).unwrap();
        assert!(out.contains(&format!(
            "export GOROOT='{}'",
            root.gos().join(v.tag()).display()
        )));
        assert!(out.contains(&format!(
            "export GOPATH='{}'",
            root.packages().join(v.tag()).join("default").display()
        )));
        assert!(out.contains("export PATH='"));
    }

    #[test]
    fn use_missing_version_fails() {
        let dir = tempfile::tempdir().unwrap();
        let root = Root::new(dir.path().join(".gvm"));
        root.ensure_dirs().unwrap();
        assert!(use_version(&root, Some("go9.9.9"), false).is_err());
    }
}
