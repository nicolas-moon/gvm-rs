use std::fs;
use std::path::Path;
use std::process::Command;

use crate::config::Root;
use crate::releases;
use crate::util;
use crate::version::GoVersion;

pub struct Opts {
    pub source_url: Option<String>,
    pub name: Option<String>,
    pub binary_only: bool,
    pub prefer_binary: bool,
}

pub fn install(root: &Root, user_version: &str, opts: &Opts) -> anyhow::Result<()> {
    root.ensure_dirs()
        .map_err(|e| anyhow::anyhow!("failed to create gvm directories: {e}"))?;
    let version = resolve_target(user_version)?;
    let gos_dir = root.gos().join(version.tag());
    if gos_dir.exists() {
        anyhow::bail!(
            "{version} is already installed at {}. Activate it with: gvm use {version}",
            gos_dir.display()
        );
    }
    let result = if opts.binary_only {
        install_binary(root, &version)
    } else if opts.prefer_binary {
        install_binary(root, &version).or_else(|e| {
            eprintln!("Binary install failed ({e}); falling back to source build.");
            install_source(root, &version, opts.source_url.as_deref())
        })
    } else {
        install_source(root, &version, opts.source_url.as_deref())
    };
    result?;
    finish_install(root, &version, opts)
}

fn finish_install(root: &Root, version: &GoVersion, opts: &Opts) -> anyhow::Result<()> {
    let name = opts.name.clone().unwrap_or_else(|| version.tag());
    crate::config::validate_name(&name)?;
    root.install_name(&name, version)?;
    // The `default` pkgset always exists per version.
    fs::create_dir_all(root.packages().join(version.tag()).join("default"))?;
    eprintln!(
        "Installed {version} at {}",
        root.gos().join(version.tag()).display()
    );
    eprintln!("Activate it with: gvm use {name}");
    Ok(())
}

/// Turn user input into a concrete version: `latest`, `tip`, exact tags, or a
/// partial like `1.21` (resolved against the go.dev release list).
fn resolve_target(user_version: &str) -> anyhow::Result<GoVersion> {
    let s = user_version.trim();
    if s.is_empty() {
        anyhow::bail!("no version given");
    }
    if s == "latest" {
        let rels = releases::fetch(true)?;
        let r = releases::latest_stable(&rels)
            .ok_or_else(|| anyhow::anyhow!("could not determine the latest stable release"))?;
        return GoVersion::parse(&r.version).map_err(|e| anyhow::anyhow!("{e}"));
    }
    let v = GoVersion::parse(s)
        .map_err(|e| anyhow::anyhow!("{e} (try e.g. 1.21, go1.21.0, latest, tip)"))?;
    if v.is_exact() {
        return Ok(v);
    }
    let (major, minor) = match &v {
        GoVersion::Release { major, minor, .. } => (*major, *minor),
        GoVersion::Tip => unreachable!(),
    };
    let rels = releases::fetch(true)?;
    let r = releases::resolve_prefix(&rels, major, minor).ok_or_else(|| {
        anyhow::anyhow!("no stable release found for go{major}.{minor} (see 'gvm listall')")
    })?;
    GoVersion::parse(&r.version).map_err(|e| anyhow::anyhow!("{e}"))
}

// --- binary installs ---

fn install_binary(root: &Root, version: &GoVersion) -> anyhow::Result<()> {
    if matches!(version, GoVersion::Tip) {
        anyhow::bail!("tip has no prebuilt binary; install it from source");
    }
    let (os, arch) = platform()?;
    let tag = version.tag();
    let filename = format!("{tag}.{os}-{arch}.tar.gz");
    let url = format!("https://go.dev/dl/{filename}");
    let tarball = root.cache().join(&filename);

    if !tarball.exists() {
        util::download(&url, &tarball)?;
    }

    // Best-effort checksum verification against go.dev metadata: resolve the
    // entry for this os/arch (kind-independent) and confirm it is exactly the
    // file we downloaded, then check its sha256.
    match releases::fetch(true).and_then(|rels| {
        let r = rels
            .into_iter()
            .find(|r| r.version == tag)
            .ok_or_else(|| anyhow::anyhow!("{tag} not present in go.dev metadata"))?;
        let f = releases::find_binary_file(&r, &os, &arch)
            .ok_or_else(|| anyhow::anyhow!("no {os}-{arch} binary in metadata for {tag}"))?;
        anyhow::ensure!(
            f.filename == filename,
            "metadata lists {} for {os}-{arch}, but we downloaded {filename}",
            f.filename
        );
        anyhow::ensure!(!f.sha256.is_empty(), "no checksum in metadata");
        let actual = util::sha256_file(&tarball)?;
        anyhow::ensure!(
            actual == f.sha256.to_ascii_lowercase(),
            "sha256 mismatch for {filename}"
        );
        Ok(f.filename)
    }) {
        Ok(verified) => eprintln!("Verified sha256 of {verified}"),
        Err(e) => eprintln!("Warning: could not verify checksum of {filename}: {e}"),
    }

    let dest = root.gos().join(&tag);
    let tmp = root.cache().join(format!(".extract-{tag}"));
    let _ = fs::remove_dir_all(&tmp);
    eprintln!("Extracting {filename}");
    util::extract_targz(&tarball, &tmp, true)?;
    if dest.exists() {
        fs::remove_dir_all(&dest)?;
    }
    fs::rename(&tmp, &dest)?;

    // Keep a clean copy in the cache for `gvm diff`.
    let clean = root.cache().join(&tag);
    if !clean.exists() {
        eprintln!("Storing a clean copy in the cache (for 'gvm diff')");
        util::extract_targz(&tarball, &clean, true)?;
    }
    Ok(())
}

// --- source installs ---

fn install_source(
    root: &Root,
    version: &GoVersion,
    source_url: Option<&str>,
) -> anyhow::Result<()> {
    match version {
        GoVersion::Tip => install_tip(root, source_url),
        _ => install_release_from_source(root, version, source_url),
    }
}

fn install_release_from_source(
    root: &Root,
    version: &GoVersion,
    source_url: Option<&str>,
) -> anyhow::Result<()> {
    let tag = version.tag();
    let filename = format!("{tag}.tar.gz");
    let tarball = root.cache().join(&filename);
    let url = source_url
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("https://go.dev/dl/{filename}"));

    if !tarball.exists() {
        util::download(&url, &tarball)?;
    }

    // Clean copy in the cache, used later by `gvm diff`.
    let clean = root.cache().join(&tag);
    if !clean.exists() {
        eprintln!("Extracting source (clean copy) into the cache");
        util::extract_targz(&tarball, &clean, true)?;
    }

    // Build in a separate copy so the clean copy stays pristine.
    let build_dir = root.cache().join(format!("build-{tag}"));
    let _ = fs::remove_dir_all(&build_dir);
    eprintln!("Preparing source build in {}", build_dir.display());
    util::extract_targz(&tarball, &build_dir, true)?;
    run_build(root, version, &build_dir)
}

fn install_tip(root: &Root, source_url: Option<&str>) -> anyhow::Result<()> {
    let tag = GoVersion::Tip.tag();
    let build_dir = root.cache().join(format!("build-{tag}"));
    let _ = fs::remove_dir_all(&build_dir);
    fs::create_dir_all(&build_dir)?;
    let repo = source_url
        .map(|s| s.to_string())
        .unwrap_or_else(|| "https://go.googlesource.com/go".to_string());
    eprintln!("Cloning {repo} (tip)");
    let status = Command::new("git")
        .args(["clone", "--depth", "1", repo.as_str()])
        .arg(&build_dir)
        .status()?;
    if !status.success() {
        anyhow::bail!("git clone of {repo} failed");
    }
    run_build(root, &GoVersion::Tip, &build_dir)
}

fn run_build(root: &Root, version: &GoVersion, build_dir: &Path) -> anyhow::Result<()> {
    let tag = version.tag();
    let make = build_dir.join("make.bash");
    if !make.is_file() {
        anyhow::bail!(
            "{} not found — is this a valid Go source tree?",
            make.display()
        );
    }
    let bootstrap = util::find_bootstrap(root, version)?;
    eprintln!(
        "Building Go {tag} (bootstrap: {}) — this can take several minutes…",
        bootstrap.display()
    );
    let bindir = bootstrap
        .parent()
        .ok_or_else(|| anyhow::anyhow!("no parent dir for bootstrap binary"))?;
    let new_path = util::prepend_path(bindir)?;
    let status = Command::new("bash")
        .arg(&make)
        .current_dir(build_dir)
        .env("GOROOT", build_dir)
        .env("PATH", new_path)
        .status()?;
    if !status.success() {
        anyhow::bail!("source build failed ({status:?})");
    }
    let dest = root.gos().join(&tag);
    if dest.exists() {
        fs::remove_dir_all(&dest)?;
    }
    fs::rename(build_dir, &dest)?;
    // Verify the freshly built toolchain.
    if let Ok(out) = Command::new(dest.join("bin").join("go"))
        .arg("version")
        .output()
    {
        if out.status.success() {
            eprintln!("{}", String::from_utf8_lossy(&out.stdout).trim());
        }
    }
    Ok(())
}

// --- (un)install, implode ---

pub fn uninstall(root: &Root, name: &str) -> anyhow::Result<()> {
    let (_, version) = root.resolve_name(name, &mut Vec::new())?;
    let tag = version.tag();
    let names = root.names_for(&version);
    root.remove_name(name);
    let _ = root.remove_alias(name);
    if names.len() <= 1 {
        // Last name for this version: remove the trees.
        for dir in [
            root.gos().join(&tag),
            root.packages().join(&tag),
            root.overlays().join(&tag),
        ] {
            if dir.exists() {
                fs::remove_dir_all(&dir)?;
            }
        }
        root.clear_default();
    }
    eprintln!("Uninstalled {version} ({name})");
    Ok(())
}

pub fn implode(root: &Root, yes: bool) -> anyhow::Result<()> {
    if root.path == util::home_dir() {
        anyhow::bail!("refusing to implode $HOME ({})", root.path.display());
    }
    if !root.path.exists() {
        println!(
            "{} does not exist; nothing to implode.",
            root.path.display()
        );
        return Ok(());
    }
    if !yes
        && !util::confirm(&format!(
            "Delete {} and all installed Go versions?",
            root.path.display()
        ))?
    {
        println!("Aborted.");
        return Ok(());
    }
    fs::remove_dir_all(&root.path)?;
    println!("Removed {}.", root.path.display());
    println!("If your shell rc file sources the gvm-rs shim, you can remove that block too.");
    Ok(())
}

/// `(os, arch)` pair in Go download naming (e.g. `linux`, `amd64`).
fn platform() -> anyhow::Result<(String, String)> {
    let (sys, machine) = util::uname()?;
    let (os, arch) = match sys.as_str() {
        "Linux" => ("linux", match machine.as_str() {
            "x86_64" | "amd64" => "amd64",
            "aarch64" | "arm64" => "arm64",
            m if m.starts_with("arm") => "arm",
            "i686" | "i386" => "386",
            other => anyhow::bail!("unsupported linux architecture: {other}"),
        }),
        "Darwin" => ("darwin", match machine.as_str() {
            "x86_64" => "amd64",
            "arm64" | "aarch64" => "arm64",
            other => anyhow::bail!("unsupported darwin architecture: {other}"),
        }),
        other => anyhow::bail!(
            "unsupported operating system: {other} (prebuilt binaries only support linux and darwin)"
        ),
    };
    Ok((os.to_string(), arch.to_string()))
}
