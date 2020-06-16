// Conserve backup system.
// Copyright 2015, 2016, 2017, 2018, 2019, 2020 Martin Pool.

//! "Apaths" (for archive paths) are platform-independent relative file paths used
//! inside archive snapshots.
//!
//! The format and semantics of apaths are defined in ../doc/format.md.
//!
//! Apaths in memory are simply strings.

use std::cmp::{Ord, Ordering, PartialEq, PartialOrd};
use std::fmt;
use std::fmt::{Display, Formatter};
use std::ops::Deref;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// An ordered archive path.
///
/// The ordering groups all the direct parents of a directory together, followed
/// by all the subdirectories.
///
/// Equal strings are equivalent to equal apaths, but the ordering is not the same as
/// string ordering.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Apath(String);

impl Apath {
    /// True if this string is a well-formed apath.
    ///
    /// Rust strings are by contract always valid UTF-8, so to meet that requirement
    /// for apaths it's enough to use a checked conversion from bytes or an `OSString`.
    pub fn is_valid(a: &str) -> bool {
        if !a.starts_with('/') {
            return false;
        } else if a.len() == 1 {
            return true;
        }
        for part in a[1..].split('/') {
            if part.is_empty() || part == "." || part == ".." || part.contains('\0') {
                return false;
            }
        }
        true
    }

    pub fn is_prefix_of(a: &str) -> Vec<&str> {
        let subtree: Vec<&str> = a.split('/').collect();
        subtree
    }
}

impl From<Apath> for String {
    fn from(a: Apath) -> String {
        a.0
    }
}

impl<'a> From<&'a Apath> for &'a str {
    fn from(a: &'a Apath) -> &'a str {
        &a.0
    }
}

impl<'a> From<&'a str> for Apath {
    fn from(s: &'a str) -> Apath {
        assert!(Apath::is_valid(s), "invalid apath: {:?}", s);
        Apath(s.to_string())
    }
}

impl From<String> for Apath {
    fn from(s: String) -> Apath {
        assert!(Apath::is_valid(&s), "invalid apath: {:?}", s);
        Apath(s)
    }
}

impl Display for Apath {
    fn fmt(&self, fmt: &mut Formatter) -> Result<(), fmt::Error> {
        write!(fmt, "{}", self.0)
    }
}

/// Compare for equality an Apath to a str.
impl PartialEq<str> for Apath {
    fn eq(&self, other: &str) -> bool {
        self.0 == *other
    }
}

impl PartialEq<&str> for Apath {
    fn eq(&self, other: &&str) -> bool {
        self.0 == **other
    }
}

impl PartialEq<Apath> for &str {
    fn eq(&self, other: &Apath) -> bool {
        other == *self
    }
}

impl Deref for Apath {
    type Target = str;
    fn deref(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for Apath {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl AsRef<Path> for Apath {
    fn as_ref(&self) -> &Path {
        self.0.as_ref()
    }
}

/// Compare two apaths.
///
/// The ordering is _not_ the same as a simple string comparison, although
/// equal strings imply equal apaths.
impl Ord for Apath {
    fn cmp(&self, b: &Apath) -> Ordering {
        let &Apath(ref a) = self;
        let &Apath(ref b) = b;
        let mut ait = a.split('/');
        let mut bit = b.split('/');
        let mut oa = ait.next().expect("paths must not be empty");
        let mut ob = bit.next().expect("paths must not be empty");
        loop {
            match (ait.next(), bit.next()) {
                // Both paths end here: eg ".../aa" < ".../zz"
                (None, None) => return oa.cmp(ob),

                // If one is a direct child and the other is in a subdirectory,
                // the direct child comes first.
                // eg ".../zz" < ".../aa/bb"
                (None, Some(_bc)) => return Ordering::Less,
                (Some(_ac), None) => return Ordering::Greater,

                // Both paths have children after this point
                (Some(ac), Some(bc)) => match oa.cmp(ob) {
                    Ordering::Equal => {
                        // a/b/c/..., a/b/c/...
                        // If parents are the same and both have children keep looking.
                        oa = ac;
                        ob = bc;
                        continue;
                    }
                    // a/b/c/... < a/b/d/...
                    // Both paths have children, but the path prefixes are
                    // different.
                    other => return other,
                },
            }
        }
    }
}

impl PartialOrd for Apath {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Observe Apaths and assert that they're visited in the correct order.
#[derive(Debug)]
pub struct CheckOrder {
    /// The last-seen filename, to enforce ordering.
    last_apath: Option<Apath>,
}

impl CheckOrder {
    pub fn new() -> CheckOrder {
        CheckOrder { last_apath: None }
    }

    pub fn check(&mut self, a: &Apath) {
        if let Some(ref last_apath) = self.last_apath {
            assert!(
                last_apath < a,
                "apaths out of order: {:?} should be before {:?}",
                last_apath,
                a
            );
        }
        self.last_apath = Some(a.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::Apath;

    #[test]
    pub fn invalid() {
        let invalid_cases = [
            "",
            "//",
            "//a",
            "/a//b",
            "/a/",
            "/a//",
            "./a/b",
            "/./a/b",
            "/a/b/.",
            "/a/./b",
            "/a/b/../c",
            "../a",
            "/hello\0",
        ];
        for v in invalid_cases.iter() {
            if Apath::is_valid(v) {
                panic!("{:?} incorrectly marked valid", v);
            }
        }
    }

    #[test]
    pub fn valid_and_ordered() {
        let ordered = [
            "/",
            "/...a",
            "/.a",
            "/a",
            "/b",
            "/kleine Katze Fuß",
            "/~~",
            "/ñ",
            "/a/...",
            "/a/..obscure",
            "/a/.config",
            "/a/1",
            "/a/100",
            "/a/2",
            "/a/añejo",
            "/a/b/c",
            "/b/((",
            "/b/,",
            "/b/A",
            "/b/AAAA",
            "/b/a",
            "/b/b",
            "/b/c",
            "/b/a/c",
            "/b/b/c",
            "/b/b/b/z",
            "/b/b/b/{zz}",
        ];
        for (i, a) in ordered.iter().enumerate() {
            if !Apath::is_valid(a) {
                panic!("{:?} incorrectly marked invalid", a);
            }
            let ap = Apath::from(*a);
            // Check it can be formatted
            assert_eq!(format!("{}", ap), *a);
            for (j, b) in ordered.iter().enumerate() {
                let expected_order = i.cmp(&j);
                let bp = Apath::from(*b);
                let r = ap.cmp(&bp);
                if r != expected_order {
                    panic!(
                        "cmp({:?}, {:?}): returned {:?} expected {:?}",
                        ap, bp, r, expected_order
                    );
                }
            }
        }
    }
}
