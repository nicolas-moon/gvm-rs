use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use crate::version::GoVersion;

/// The gvm data directory: `~/.gvm` by default, overridable with `$GVM_ROOT`.
///
/// Layout (mirrors the original gvm):
/// ```text
/// .gvm/
///   gos/<version>/          installed GOROOTs
///   packages/<version>/<pkgset>/   GOPATHs
///   cache/                  downloaded tarballs + clean copies + build trees
///   versions/<name>         one file per installed name; content = version tag
///   config/default          name of the default version
///   config/alias            `alias name  ->  version name` lines
///   scripts/gvm             the sourced shell shim
///   overlays/<version>/     vendored native-deps prefix (GVM_OVERLAY_PREFIX)
///   bin/                    where the binary lives (used by the shim)
///   logs/                   build logs
/// ```
#[derive(Debug, Clone)]
pub struct Root {
    pub path: PathBuf,
}

impl Root {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn from_env() -> Self {
        if let Ok(p) = std::env::var("GVM_ROOT") {
            if !p.is_empty() {
                return Self::new(PathBuf::from(p));
            }
        }
        Self::new(crate::util::home_dir().join(".gvm"))
    }

    pub fn ensure_dirs(&self) -> std::io::Result<()> {
        for d in [
            self.gos(),
            self.packages(),
            self.cache(),
            self.versions(),
            self.config_dir(),
            self.scripts_dir(),
            self.overlays(),
            self.bin_dir(),
            self.logs(),
        ] {
            fs::create_dir_all(&d)?;
        }
        Ok(())
    }

    pub fn gos(&self) -> PathBuf {
        self.path.join("gos")
    }
    pub fn packages(&self) -> PathBuf {
        self.path.join("packages")
    }
    pub fn cache(&self) -> PathBuf {
        self.path.join("cache")
    }
    pub fn versions(&self) -> PathBuf {
        self.path.join("versions")
    }
    pub fn config_dir(&self) -> PathBuf {
        self.path.join("config")
    }
    pub fn scripts_dir(&self) -> PathBuf {
        self.path.join("scripts")
    }
    pub fn overlays(&self) -> PathBuf {
        self.path.join("overlays")
    }
    pub fn bin_dir(&self) -> PathBuf {
        self.path.join("bin")
    }
    pub fn logs(&self) -> PathBuf {
        self.path.join("logs")
    }

    // --- default version ---

    fn default_file(&self) -> PathBuf {
        self.config_dir().join("default")
    }

    pub fn default_name(&self) -> Option<String> {
        fs::read_to_string(self.default_file())
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    }

    pub fn set_default(&self, name: &str) -> anyhow::Result<()> {
        fs::create_dir_all(self.config_dir())?;
        fs::write(self.default_file(), format!("{name}\n"))?;
        Ok(())
    }

    pub fn clear_default(&self) {
        let _ = fs::remove_file(self.default_file());
    }

    // --- aliases ---

    fn alias_file(&self) -> PathBuf {
        self.config_dir().join("alias")
    }

    pub fn aliases(&self) -> BTreeMap<String, String> {
        let Ok(content) = fs::read_to_string(self.alias_file()) else {
            return BTreeMap::new();
        };
        let mut out = BTreeMap::new();
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut it = line.split_whitespace();
            let (Some(name), Some(target)) = (it.next(), it.next()) else {
                continue;
            };
            out.insert(name.to_string(), target.to_string());
        }
        out
    }

    pub fn set_alias(&self, name: &str, target: &str) -> anyhow::Result<()> {
        let mut map = self.aliases();
        map.insert(name.to_string(), target.to_string());
        self.write_alias_file(&map)
    }

    pub fn remove_alias(&self, name: &str) -> anyhow::Result<bool> {
        let mut map = self.aliases();
        let removed = map.remove(name).is_some();
        if removed {
            self.write_alias_file(&map)?;
        }
        Ok(removed)
    }

    fn write_alias_file(&self, map: &BTreeMap<String, String>) -> anyhow::Result<()> {
        fs::create_dir_all(self.config_dir())?;
        let mut s = String::from("# alias name -> version name\n");
        for (k, v) in map {
            s.push_str(k);
            s.push(' ');
            s.push_str(v);
            s.push('\n');
        }
        fs::write(self.alias_file(), s)
            .map_err(|e| anyhow::anyhow!("failed to write alias file: {e}"))
    }

    // --- installed version names ---

    pub fn name_file(&self, name: &str) -> PathBuf {
        self.versions().join(name)
    }

    pub fn version_of_name(&self, name: &str) -> Option<GoVersion> {
        fs::read_to_string(self.name_file(name))
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .and_then(|tag| GoVersion::parse(&tag).ok())
    }

    pub fn install_name(&self, name: &str, version: &GoVersion) -> anyhow::Result<()> {
        fs::create_dir_all(self.versions())?;
        fs::write(self.name_file(name), format!("{}\n", version.tag()))?;
        Ok(())
    }

    pub fn remove_name(&self, name: &str) {
        let _ = fs::remove_file(self.name_file(name));
    }

    /// All installed `(name, version)` pairs, newest version first.
    pub fn installed(&self) -> Vec<(String, GoVersion)> {
        let Ok(entries) = fs::read_dir(self.versions()) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if let Ok(content) = fs::read_to_string(e.path()) {
                if let Ok(v) = GoVersion::parse(content.trim()) {
                    out.push((name, v));
                }
            }
        }
        out.sort_by(|a, b| b.1.cmp(&a.1));
        out
    }

    /// All names that resolve to the given version.
    pub fn names_for(&self, version: &GoVersion) -> Vec<String> {
        self.installed()
            .into_iter()
            .filter(|(_, v)| v == version)
            .map(|(n, _)| n)
            .collect()
    }

    /// Resolve a user-supplied name to `(name, version)`, following aliases
    /// (with a cycle guard). A name that is neither a known installed name nor
    /// an alias is parsed as a version tag itself.
    pub fn resolve_name(
        &self,
        name: &str,
        seen: &mut Vec<String>,
    ) -> anyhow::Result<(String, GoVersion)> {
        if seen.iter().any(|s| s == name) {
            anyhow::bail!("alias cycle involving {name:?}");
        }
        seen.push(name.to_string());
        if let Some(v) = self.version_of_name(name) {
            return Ok((name.to_string(), v));
        }
        if let Some(target) = self.aliases().get(name) {
            return self.resolve_name(target, seen);
        }
        let v = GoVersion::parse(name).map_err(|e| anyhow::anyhow!("{e}"))?;
        Ok((v.tag(), v))
    }

    /// The version active in this shell (from `$GOROOT`), when it is a
    /// gvm-managed GOROOT.
    pub fn active_version(&self) -> Option<GoVersion> {
        let goroot = std::env::var("GOROOT").ok().filter(|s| !s.is_empty())?;
        let p = PathBuf::from(goroot);
        let dir = p.file_name()?.to_str()?;
        let parent = p.parent()?;
        if parent != self.gos() {
            return None;
        }
        GoVersion::parse(dir)
            .ok()
            .filter(|v| self.gos().join(v.tag()).is_dir())
    }
}

/// Validate a name for versions / pkgsets (no path tricks, sane charset).
pub fn validate_name(name: &str) -> anyhow::Result<()> {
    if name.is_empty() || name == "." || name == ".." {
        anyhow::bail!("invalid name: {name:?}");
    }
    if name.contains('/') || name.contains('\\') {
        anyhow::bail!("name must not contain path separators: {name:?}");
    }
    let ok = name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '+'));
    if !ok {
        anyhow::bail!("name contains invalid characters: {name:?}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_root(tag: &str) -> (tempfile::TempDir, Root) {
        let dir = tempfile::tempdir().unwrap();
        let root = Root::new(dir.path().join(".gvm"));
        root.ensure_dirs().unwrap();
        let _ = tag;
        (dir, root)
    }

    #[test]
    fn resolve_name_direct() {
        let (_dir, root) = tmp_root("x");
        let v = GoVersion::parse("go1.21.0").unwrap();
        root.install_name("go1.21.0", &v).unwrap();
        let (name, version) = root.resolve_name("go1.21.0", &mut Vec::new()).unwrap();
        assert_eq!(name, "go1.21.0");
        assert_eq!(version, v);
    }

    #[test]
    fn resolve_name_follows_alias() {
        let (_dir, root) = tmp_root("x");
        let v = GoVersion::parse("go1.21.0").unwrap();
        root.install_name("go1.21.0", &v).unwrap();
        root.set_alias("stable", "go1.21.0").unwrap();
        let (name, version) = root.resolve_name("stable", &mut Vec::new()).unwrap();
        assert_eq!(name, "go1.21.0");
        assert_eq!(version, v);
    }

    #[test]
    fn resolve_name_rejects_alias_cycle() {
        let (_dir, root) = tmp_root("x");
        root.set_alias("a", "b").unwrap();
        root.set_alias("b", "a").unwrap();
        assert!(root.resolve_name("a", &mut Vec::new()).is_err());
    }

    #[test]
    fn resolve_name_falls_back_to_tag() {
        let (_dir, root) = tmp_root("x");
        let (name, version) = root.resolve_name("go1.20.1", &mut Vec::new()).unwrap();
        assert_eq!(name, "go1.20.1");
        assert_eq!(version, GoVersion::parse("go1.20.1").unwrap());
    }

    #[test]
    fn validate_name_rules() {
        assert!(validate_name("go1.21.0").is_ok());
        assert!(validate_name("my-pkgset_2").is_ok());
        assert!(validate_name("").is_err());
        assert!(validate_name("..").is_err());
        assert!(validate_name("a/b").is_err());
        assert!(validate_name("a b").is_err());
    }

    #[test]
    fn default_roundtrip() {
        let (_dir, root) = tmp_root("x");
        assert_eq!(root.default_name(), None);
        root.set_default("go1.21.0").unwrap();
        assert_eq!(root.default_name().as_deref(), Some("go1.21.0"));
        root.clear_default();
        assert_eq!(root.default_name(), None);
    }
}
