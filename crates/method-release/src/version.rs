//! Semantic versions: `MAJOR.MINOR.PATCH`, optionally `-PRERELEASE`
//! (dot-separated ASCII letters, digits and hyphens). No build metadata:
//! nothing here needs it, and a `+` in a tag or a file name is trouble.

use std::cmp::Ordering;
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    /// The prerelease identifiers (`beta.2` is `["beta", "2"]`); empty for a
    /// release.
    pub pre: Vec<String>,
}

/// Which part of the version a bump raises.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    Major,
    Minor,
    Patch,
}

fn number(s: &str, what: &str, whole: &str) -> Result<u64, String> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return Err(format!(
            "\"{whole}\" is not MAJOR.MINOR.PATCH: the {what} part is not a number"
        ));
    }
    if s.len() > 1 && s.starts_with('0') {
        return Err(format!("\"{whole}\" has a leading zero in its {what} part"));
    }
    s.parse()
        .map_err(|_| format!("\"{whole}\" has a {what} part too big to be a version"))
}

impl Version {
    /// Parses `1.2.3` or `1.2.3-beta.1`. A leading `v` is refused: tags
    /// carry it, versions don't, and [`Version::from_tag`] is the door for
    /// tags.
    pub fn parse(s: &str) -> Result<Version, String> {
        let (core, pre) = match s.split_once('-') {
            Some((c, p)) => (c, Some(p)),
            None => (s, None),
        };
        let parts: Vec<&str> = core.split('.').collect();
        if parts.len() != 3 {
            return Err(format!("\"{s}\" is not MAJOR.MINOR.PATCH"));
        }
        let pre = match pre {
            None => Vec::new(),
            Some(p) => {
                let ids: Vec<String> = p.split('.').map(str::to_string).collect();
                for id in &ids {
                    let ok = !id.is_empty()
                        && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-');
                    if !ok {
                        return Err(format!(
                            "\"{s}\" has a prerelease part that isn't dot-separated letters, digits and hyphens"
                        ));
                    }
                    if id.bytes().all(|b| b.is_ascii_digit()) && id.len() > 1 && id.starts_with('0')
                    {
                        return Err(format!("\"{s}\" has a leading zero in its prerelease part"));
                    }
                }
                ids
            }
        };
        Ok(Version {
            major: number(parts[0], "major", s)?,
            minor: number(parts[1], "minor", s)?,
            patch: number(parts[2], "patch", s)?,
            pre,
        })
    }

    /// Parses a release tag, `v1.2.3` (the `v` is required).
    pub fn from_tag(tag: &str) -> Result<Version, String> {
        let rest = tag
            .strip_prefix('v')
            .ok_or_else(|| format!("tag \"{tag}\" doesn't start with v (like v0.1.0)"))?;
        Version::parse(rest)
    }

    pub fn is_prerelease(&self) -> bool {
        !self.pre.is_empty()
    }

    /// The next version after raising `part`: the parts after it go to
    /// zero and any prerelease is dropped. A prerelease of the very version
    /// a bump lands on is finished instead (`1.0.0-beta.1` and a major bump
    /// make `1.0.0`), as semantic versioning orders them.
    pub fn bump(&self, part: Part) -> Version {
        let pre_of = |v: Version| -> Version {
            if self.is_prerelease()
                && v.major == self.major
                && v.minor == self.minor
                && v.patch == self.patch
            {
                Version { pre: vec![], ..v }
            } else {
                v
            }
        };
        let plain = |major, minor, patch| Version {
            major,
            minor,
            patch,
            pre: vec![],
        };
        match part {
            Part::Major => {
                if self.is_prerelease() && self.minor == 0 && self.patch == 0 {
                    pre_of(plain(self.major, 0, 0))
                } else {
                    plain(self.major + 1, 0, 0)
                }
            }
            Part::Minor => {
                if self.is_prerelease() && self.patch == 0 {
                    pre_of(plain(self.major, self.minor, 0))
                } else {
                    plain(self.major, self.minor + 1, 0)
                }
            }
            Part::Patch => {
                if self.is_prerelease() {
                    pre_of(plain(self.major, self.minor, self.patch))
                } else {
                    plain(self.major, self.minor, self.patch + 1)
                }
            }
        }
    }

    /// The release tag for this version: `v1.2.3`.
    pub fn tag(&self) -> String {
        format!("v{self}")
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if !self.pre.is_empty() {
            write!(f, "-{}", self.pre.join("."))?;
        }
        Ok(())
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Version {
    /// Semantic versioning precedence: a prerelease sorts before its
    /// release; numeric identifiers compare as numbers and sort before
    /// alphanumeric ones; a longer list of identifiers wins a tie.
    fn cmp(&self, other: &Self) -> Ordering {
        (self.major, self.minor, self.patch)
            .cmp(&(other.major, other.minor, other.patch))
            .then_with(|| match (self.pre.is_empty(), other.pre.is_empty()) {
                (true, true) => Ordering::Equal,
                (true, false) => Ordering::Greater,
                (false, true) => Ordering::Less,
                (false, false) => {
                    for (a, b) in self.pre.iter().zip(&other.pre) {
                        let ord = match (a.parse::<u64>(), b.parse::<u64>()) {
                            (Ok(x), Ok(y)) => x.cmp(&y),
                            (Ok(_), Err(_)) => Ordering::Less,
                            (Err(_), Ok(_)) => Ordering::Greater,
                            (Err(_), Err(_)) => a.cmp(b),
                        };
                        if ord != Ordering::Equal {
                            return ord;
                        }
                    }
                    self.pre.len().cmp(&other.pre.len())
                }
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &str) -> Version {
        Version::parse(s).unwrap()
    }

    #[test]
    fn parses_and_prints() {
        assert_eq!(v("0.1.0").to_string(), "0.1.0");
        assert_eq!(v("1.20.3-beta.2").to_string(), "1.20.3-beta.2");
        assert_eq!(v("1.2.3-rc-1").pre, vec!["rc-1"]);
        assert_eq!(Version::from_tag("v0.1.0").unwrap(), v("0.1.0"));
        assert_eq!(v("0.1.0").tag(), "v0.1.0");
    }

    #[test]
    fn refuses_what_isnt_a_version() {
        for bad in [
            "",
            "1",
            "1.2",
            "1.2.3.4",
            "v1.2.3",
            "01.2.3",
            "1.02.3",
            "1.2.x",
            "1.2.3-",
            "1.2.3-beta..1",
            "1.2.3-be ta",
            "1.2.3-01",
            "1.2.3+build",
            " 1.2.3",
        ] {
            assert!(Version::parse(bad).is_err(), "{bad:?} should be refused");
        }
        assert!(Version::from_tag("0.1.0").is_err());
        assert!(Version::from_tag("main").is_err());
    }

    #[test]
    fn orders_like_semver() {
        let order = [
            "0.0.1",
            "0.1.0-alpha",
            "0.1.0-alpha.1",
            "0.1.0-alpha.beta",
            "0.1.0-beta",
            "0.1.0-beta.2",
            "0.1.0-beta.11",
            "0.1.0-rc.1",
            "0.1.0",
            "0.1.1",
            "0.2.0",
            "1.0.0",
            "1.10.0",
        ];
        for pair in order.windows(2) {
            assert!(v(pair[0]) < v(pair[1]), "{} < {}", pair[0], pair[1]);
        }
    }

    #[test]
    fn bumps() {
        assert_eq!(v("0.1.0").bump(Part::Patch), v("0.1.1"));
        assert_eq!(v("0.1.7").bump(Part::Minor), v("0.2.0"));
        assert_eq!(v("0.9.7").bump(Part::Major), v("1.0.0"));
        // A prerelease is finished by the bump that lands on it.
        assert_eq!(v("1.0.0-beta.1").bump(Part::Major), v("1.0.0"));
        assert_eq!(v("0.3.0-rc.1").bump(Part::Minor), v("0.3.0"));
        assert_eq!(v("0.3.2-rc.1").bump(Part::Patch), v("0.3.2"));
        // ...and passed over by one that doesn't.
        assert_eq!(v("0.3.2-rc.1").bump(Part::Minor), v("0.4.0"));
        for s in ["0.1.0", "1.0.0-beta.1", "0.3.2-rc.1"] {
            for p in [Part::Major, Part::Minor, Part::Patch] {
                assert!(v(s).bump(p) > v(s), "{s} bumped by {p:?} must go up");
            }
        }
    }
}
