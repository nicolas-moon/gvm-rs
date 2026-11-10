/// A Go toolchain version: a release tag (e.g. `go1.21.0`) or `tip`
/// (the head of the Go master branch).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GoVersion {
    Release {
        major: u32,
        minor: u32,
        patch: Option<u32>,
        prerelease: Option<String>,
    },
    Tip,
}

impl GoVersion {
    /// Parse a version as users type it: `go1.21.0`, `1.21.0`, `1.21`,
    /// `1.22rc1`, `1.22-beta1`, `tip`. `latest` is handled by callers.
    pub fn parse(input: &str) -> Result<Self, String> {
        let s = input.trim();
        let s = s.strip_prefix("go").unwrap_or(s);
        if s == "tip" {
            return Ok(GoVersion::Tip);
        }
        if s.is_empty() {
            return Err(format!("invalid Go version: {input:?}"));
        }
        // Core numeric part: digits and dots, up to the first other character
        // (a prerelease suffix like `rc1` or `-beta1`).
        let core_end = s
            .find(|c: char| !c.is_ascii_digit() && c != '.')
            .unwrap_or(s.len());
        let core = &s[..core_end];
        let rest = &s[core_end..];
        let parts: Vec<&str> = core.split('.').collect();
        if !(2..=3).contains(&parts.len())
            || parts
                .iter()
                .any(|p| p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()))
        {
            return Err(format!("invalid Go version: {input:?}"));
        }
        let major = parts[0]
            .parse()
            .map_err(|e: std::num::ParseIntError| e.to_string())?;
        let minor = parts[1]
            .parse()
            .map_err(|e: std::num::ParseIntError| e.to_string())?;
        let patch = parts
            .get(2)
            .map(|p| p.parse())
            .transpose()
            .map_err(|e: std::num::ParseIntError| e.to_string())?;
        let prerelease = if rest.is_empty() {
            None
        } else {
            Some(rest.to_string())
        };
        Ok(GoVersion::Release {
            major,
            minor,
            patch,
            prerelease,
        })
    }

    /// Canonical tag, e.g. `go1.21.0`, `go1.22rc1`, `tip`.
    pub fn tag(&self) -> String {
        match self {
            GoVersion::Tip => "tip".to_string(),
            GoVersion::Release {
                major,
                minor,
                patch,
                prerelease,
            } => {
                let mut s = format!("go{major}.{minor}");
                if let Some(p) = patch {
                    s.push('.');
                    s.push_str(&p.to_string());
                }
                if let Some(pre) = prerelease {
                    s.push_str(pre);
                }
                s
            }
        }
    }

    /// A released, non-prerelease version with a patch component.
    #[allow(dead_code)] // part of the public version model; exercised in tests
    pub fn is_stable(&self) -> bool {
        matches!(
            self,
            GoVersion::Release {
                patch: Some(_),
                prerelease: None,
                ..
            }
        )
    }

    /// `true` when this pin is already exact (no version-list lookup needed):
    /// a full patch version, a prerelease tag, or `tip`.
    pub fn is_exact(&self) -> bool {
        match self {
            GoVersion::Tip => true,
            GoVersion::Release {
                patch, prerelease, ..
            } => patch.is_some() || prerelease.is_some(),
        }
    }

    fn sort_key(&self) -> (u64, u32, u32, u32, u8, String) {
        match self {
            GoVersion::Tip => (u64::MAX, 0, 0, 0, 0, String::new()),
            GoVersion::Release {
                major,
                minor,
                patch,
                prerelease,
            } => (
                0,
                *major,
                *minor,
                patch.unwrap_or(0),
                if prerelease.is_some() { 0 } else { 1 },
                prerelease.clone().unwrap_or_default(),
            ),
        }
    }
}

impl PartialOrd for GoVersion {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for GoVersion {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.sort_key().cmp(&other.sort_key())
    }
}

impl std::fmt::Display for GoVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.tag())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_release_forms() {
        let v = GoVersion::parse("go1.21.0").unwrap();
        assert_eq!(v.tag(), "go1.21.0");
        assert!(v.is_stable());
        assert!(v.is_exact());

        let v = GoVersion::parse("1.21.0").unwrap();
        assert_eq!(v.tag(), "go1.21.0");

        let v = GoVersion::parse("go1.21").unwrap();
        assert_eq!(v.tag(), "go1.21");
        assert!(!v.is_exact());

        let v = GoVersion::parse("1.22rc1").unwrap();
        assert_eq!(v.tag(), "go1.22rc1");
        assert!(v.is_exact());
        assert!(!v.is_stable());

        let v = GoVersion::parse("go1.22-beta1").unwrap();
        assert_eq!(v.tag(), "go1.22-beta1");
    }

    #[test]
    fn parses_tip() {
        let v = GoVersion::parse("tip").unwrap();
        assert_eq!(v, GoVersion::Tip);
        assert!(v.is_exact());
    }

    #[test]
    fn rejects_garbage() {
        assert!(GoVersion::parse("").is_err());
        assert!(GoVersion::parse("hello").is_err());
        assert!(GoVersion::parse("1").is_err());
        assert!(GoVersion::parse("1.2.3.4").is_err());
        assert!(GoVersion::parse("latest").is_err());
    }

    #[test]
    fn ordering() {
        let a = GoVersion::parse("1.20.5").unwrap();
        let c = GoVersion::parse("1.21.0").unwrap();
        let d = GoVersion::parse("1.22rc1").unwrap();
        let tip = GoVersion::Tip;
        assert!(a < c);
        assert!(c < d);
        assert!(d < tip);
    }
}
