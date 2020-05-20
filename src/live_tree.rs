// Conserve backup system.
// Copyright 2015, 2016, 2017, 2018, 2019, 2020 Martin Pool.

//! Find source files within a source directory, in apath order.

use std::collections::vec_deque::VecDeque;
use std::fmt;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use snafu::ResultExt;

use globset::GlobSet;

use super::*;

/// A real tree on the filesystem, for use as a backup source or restore destination.
#[derive(Clone)]
pub struct LiveTree {
    path: PathBuf,
    report: Report,
    excludes: GlobSet,
}

impl LiveTree {
    pub fn open<P: AsRef<Path>>(path: P, report: &Report) -> Result<LiveTree> {
        // TODO: Maybe fail here if the root doesn't exist or isn't a directory?
        Ok(LiveTree {
            path: path.as_ref().to_path_buf(),
            report: report.clone(),
            excludes: excludes::excludes_nothing(),
        })
    }

    /// Return a new LiveTree which when listed will ignore certain files.
    ///
    /// This replaces any previous exclusions.
    pub fn with_excludes(self, excludes: GlobSet) -> LiveTree {
        LiveTree { excludes, ..self }
    }

    fn relative_path(&self, apath: &Apath) -> PathBuf {
        relative_path(&self.path, apath)
    }
}

/// An in-memory Entry describing a file/dir/symlink in a live tree.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct LiveEntry {
    apath: Apath,
    kind: Kind,
    mtime: SystemTime,
    size: Option<u64>,
    symlink_target: Option<String>,
}

fn relative_path(root: &PathBuf, apath: &Apath) -> PathBuf {
    let mut path = root.clone();
    path.push(&apath[1..]);
    path
}

impl tree::ReadTree for LiveTree {
    type Entry = LiveEntry;
    type I = Iter;
    type R = std::fs::File;

    /// Iterate source files descending through a source directory.
    ///
    /// Visit the files in a directory before descending into its children, as
    /// is the defined order for files stored in an archive.  Within those files and
    /// child directories, visit them according to a sorted comparison by their UTF-8
    /// name.
    ///
    /// The `Iter` has its own `Report` of how many directories and files were visited.
    fn iter_entries(&self, report: &Report) -> Result<Self::I> {
        Iter::new(&self.path, &self.excludes, report)
    }

    fn file_contents(&self, entry: &LiveEntry) -> Result<Self::R> {
        assert_eq!(entry.kind(), Kind::File);
        let path = self.relative_path(&entry.apath);
        fs::File::open(&path).context(errors::ReadSourceFile { path })
    }

    fn estimate_count(&self) -> Result<u64> {
        // TODO: This stats the file and builds an entry about them, just to
        // throw it away. We could perhaps change the iter to optionally do
        // less work.

        // Make a new report so it doesn't pollute the report for the actual
        // backup work.
        Ok(self.iter_entries(&Report::new())?.count() as u64)
    }
}

impl HasReport for LiveTree {
    fn report(&self) -> &Report {
        &self.report
    }
}

impl fmt::Debug for LiveTree {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("LiveTree")
            .field("path", &self.path)
            .finish()
    }
}

impl Entry for LiveEntry {
    fn apath(&self) -> &Apath {
        &self.apath
    }

    fn kind(&self) -> Kind {
        self.kind
    }

    fn mtime(&self) -> SystemTime {
        self.mtime
    }

    fn size(&self) -> Option<u64> {
        self.size
    }

    fn symlink_target(&self) -> &Option<String> {
        &self.symlink_target
    }
}

impl LiveEntry {
    fn from_fs_metadata(
        apath: Apath,
        metadata: &fs::Metadata,
        symlink_target: Option<String>,
    ) -> LiveEntry {
        // TODO: Could we read the symlink target here, rather than in the caller?
        let kind = if metadata.is_file() {
            Kind::File
        } else if metadata.is_dir() {
            Kind::Dir
        } else if metadata.file_type().is_symlink() {
            Kind::Symlink
        } else {
            Kind::Unknown
        };
        let mtime = metadata.modified().expect("Failed to get file mtime");
        let size = if metadata.is_file() {
            Some(metadata.len())
        } else {
            None
        };
        LiveEntry {
            apath,
            kind,
            mtime,
            symlink_target,
            size,
        }
    }
}

/// Recursive iterator of the contents of a live tree.
#[derive(Debug)]
pub struct Iter {
    /// Root of the source tree.
    root_path: PathBuf,

    /// Directories yet to be visited.
    dir_deque: VecDeque<Apath>,

    /// All entries that have been seen but not yet returned by the iterator, in the order they
    /// should be returned.
    entry_deque: VecDeque<LiveEntry>,

    /// Count of directories and files visited by this iterator.
    report: Report,

    /// Check that emitted paths are in the right order.
    check_order: apath::CheckOrder,

    /// glob pattern to skip in iterator
    excludes: GlobSet,
}

impl Iter {
    /// Construct a new iter that will visit everything below this root path,
    /// subject to some exclusions
    fn new(root_path: &Path, excludes: &GlobSet, report: &Report) -> Result<Iter> {
        let root_metadata = fs::symlink_metadata(&root_path)
            .with_context(|| errors::ListSourceTree {
                path: root_path.to_path_buf(),
            })
            .map_err(|e| {
                report.show_error(&e);
                e
            })?;
        // Preload iter to return the root and then recurse into it.
        let mut entry_deque = VecDeque::<LiveEntry>::new();
        entry_deque.push_back(LiveEntry::from_fs_metadata(
            Apath::from("/"),
            &root_metadata,
            None,
        ));
        // TODO: Consider the case where the root is not actually a directory?
        // Should that be supported?
        let mut dir_deque = VecDeque::<Apath>::new();
        dir_deque.push_back("/".into());
        Ok(Iter {
            root_path: root_path.to_path_buf(),
            entry_deque,
            dir_deque,
            report: report.clone(),
            check_order: apath::CheckOrder::new(),
            excludes: excludes.clone(),
        })
    }

    /// Visit the next directory.
    ///
    /// Any errors occurring are logged but not returned; we'll continue to
    /// visit whatever can be read.
    fn visit_next_directory(&mut self, parent_apath: &Apath) {
        // TODO: Rather than mutating self, return new vectors to append, so that
        // this function isn't too big?
        //
        // TODO: Perhaps reuse the child buffer in the Iter, which we know will
        // now be empty? We have to be able to sort it, but perhaps a Vec in
        // reverse order from which we pop would work well.
        self.report.increment("source.visited.directories", 1);
        let mut children = Vec::<(String, LiveEntry)>::new();
        let dir_path = relative_path(&self.root_path, parent_apath);
        let dir_iter = match fs::read_dir(&dir_path).with_context(|| errors::ListSourceTree {
            path: dir_path.clone(),
        }) {
            Ok(i) => i,
            Err(e) => {
                self.report
                    .problem(&format!("Error reading directory {:?}: {}", &dir_path, e));
                return;
            }
        };
        for dir_entry in dir_iter {
            let dir_entry = match dir_entry {
                Ok(dir_entry) => dir_entry,
                Err(e) => {
                    self.report.problem(&format!(
                        "Error reading next entry from directory {:?}: {}",
                        &dir_path, e
                    ));
                    continue;
                }
            };
            let mut child_apath_str = parent_apath.to_string();
            // TODO: Specific Apath join method?
            if child_apath_str != "/" {
                child_apath_str.push('/');
            }
            let child_osstr = &dir_entry.file_name();
            let child_name = match child_osstr.to_str() {
                Some(c) => c,
                None => {
                    self.report.problem(&format!(
                        "Can't decode filename {:?} in {:?}",
                        child_osstr, dir_path,
                    ));
                    continue;
                }
            };
            child_apath_str.push_str(child_name);
            let ft = match dir_entry.file_type() {
                Ok(ft) => ft,
                Err(e) => {
                    self.report.problem(&format!(
                        "Error getting type of {:?} during iteration: {}",
                        child_apath_str, e
                    ));
                    continue;
                }
            };

            if self.excludes.is_match(&child_apath_str) {
                if ft.is_file() {
                    self.report.increment("skipped.excluded.files", 1);
                } else if ft.is_dir() {
                    self.report.increment("skipped.excluded.directories", 1);
                } else if ft.is_symlink() {
                    self.report.increment("skipped.excluded.symlinks", 1);
                }
                continue;
            }
            let metadata = match dir_entry.metadata() {
                Ok(metadata) => metadata,
                Err(e) => {
                    match e.kind() {
                        ErrorKind::NotFound => {
                            // Fairly harmless, and maybe not even worth logging. Just a race
                            // between listing the directory and looking at the contents.
                            self.report.problem(&format!(
                                "File disappeared during iteration: {:?}: {}",
                                child_apath_str, e
                            ));
                        }
                        _ => {
                            self.report.problem(&format!(
                                "Failed to read source metadata from {:?}: {}",
                                child_apath_str, e
                            ));
                            self.report.increment("source.error.metadata", 1);
                        }
                    };
                    continue;
                }
            };

            // TODO: Move this into LiveEntry::from_fs_metadata, once there's a
            // global way for it to complain about errors.
            let target: Option<String> = if ft.is_symlink() {
                let t = match dir_path.join(dir_entry.file_name()).read_link() {
                    Ok(t) => t,
                    Err(e) => {
                        self.report.problem(&format!(
                            "Failed to read target of symlink {:?}: {}",
                            child_apath_str, e
                        ));
                        continue;
                    }
                };
                match t.into_os_string().into_string() {
                    Ok(t) => Some(t),
                    Err(e) => {
                        self.report.problem(&format!(
                            "Failed to decode target of symlink {:?}: {:?}",
                            child_apath_str, e
                        ));
                        continue;
                    }
                }
            } else {
                None
            };
            children.push((
                child_name.to_string(),
                LiveEntry::from_fs_metadata(child_apath_str.into(), &metadata, target),
            ));
        }
        children.sort_unstable_by(|a, b| a.0.cmp(&b.0));
        // To get the right overall tree ordering, any new subdirectories
        // discovered here should be visited together in apath order, but before
        // any previously pending directories. In other words, in reverse order
        // push them onto the front of the dir deque.
        for idir in children.iter().filter(|x| x.1.kind == Kind::Dir).rev() {
            self.dir_deque.push_front(idir.1.apath().clone())
        }
        self.entry_deque.reserve(children.len());
        self.entry_deque.extend(children.into_iter().map(|x| x.1));
    }
}

// The source iterator yields one path at a time as it walks through the source directories.
//
// It has to read each directory entirely so that it can sort the entries.
// These entries are then returned before visiting any subdirectories.
//
// It also has to manage a stack of directories which might be partially walked.  Those
// subdirectories are then visited, also in sorted order, before returning to
// any higher-level directories.
impl Iterator for Iter {
    type Item = LiveEntry;

    fn next(&mut self) -> Option<LiveEntry> {
        loop {
            if let Some(entry) = self.entry_deque.pop_front() {
                // Have already found some entries, so just return the first.
                self.report.increment("source.selected", 1);
                // Sanity check that all the returned paths are in correct order.
                self.check_order.check(&entry.apath);
                return Some(entry);
            } else if let Some(entry) = self.dir_deque.pop_front() {
                // No entries already queued, visit a new directory to try to refill the queue.
                self.visit_next_directory(&entry)
            } else {
                // No entries queued and no more directories to visit.
                return None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::*;
    use crate::test_fixtures::TreeFixture;

    use regex::Regex;

    #[test]
    fn open_tree() {
        let tf = TreeFixture::new();
        let lt = LiveTree::open(tf.path(), &Report::new()).unwrap();
        assert_eq!(
            format!("{:?}", &lt),
            format!("LiveTree {{ path: {:?} }}", tf.path())
        );
    }

    #[test]
    fn simple_directory() {
        let tf = TreeFixture::new();
        tf.create_file("bba");
        tf.create_file("aaa");
        tf.create_dir("jam");
        tf.create_file("jam/apricot");
        tf.create_dir("jelly");
        tf.create_dir("jam/.etc");
        let report = Report::new();
        let lt = LiveTree::open(tf.path(), &report).unwrap();
        let mut source_iter = lt.iter_entries(&report).unwrap();
        let result = source_iter.by_ref().collect::<Vec<_>>();
        // First one is the root
        assert_eq!(&result[0].apath, "/");
        assert_eq!(&result[1].apath, "/aaa");
        assert_eq!(&result[2].apath, "/bba");
        assert_eq!(&result[3].apath, "/jam");
        assert_eq!(&result[4].apath, "/jelly");
        assert_eq!(&result[5].apath, "/jam/.etc");
        assert_eq!(&result[6].apath, "/jam/apricot");
        assert_eq!(result.len(), 7);

        let repr = format!("{:?}", &result[6]);
        let re = Regex::new(r#"LiveEntry \{ apath: Apath\("/jam/apricot"\), kind: File, mtime: SystemTime \{ [^)]* \}, size: Some\(8\), symlink_target: None \}"#).unwrap();
        assert!(re.is_match(&repr), repr);

        assert_eq!(report.get_count("source.visited.directories"), 4);
        assert_eq!(report.get_count("source.selected"), 7);
    }

    #[test]
    fn exclude_entries_directory() {
        let tf = TreeFixture::new();
        tf.create_file("foooo");
        tf.create_file("bar");
        tf.create_dir("fooooBar");
        tf.create_dir("baz");
        tf.create_file("baz/bar");
        tf.create_file("baz/bas");
        tf.create_file("baz/test");
        let report = Report::new();

        let excludes = excludes::from_strings(&["/**/fooo*", "/**/ba[pqr]", "/**/*bas"]).unwrap();

        let lt = LiveTree::open(tf.path(), &report)
            .unwrap()
            .with_excludes(excludes);
        let mut source_iter = lt.iter_entries(&report).unwrap();
        let result = source_iter.by_ref().collect::<Vec<_>>();

        // First one is the root
        assert_eq!(&result[0].apath, "/");
        assert_eq!(&result[1].apath, "/baz");
        assert_eq!(&result[2].apath, "/baz/test");
        assert_eq!(result.len(), 3);

        assert_eq!(
            2,
            report
                .borrow_counts()
                .get_count("source.visited.directories",)
        );
        assert_eq!(3, report.borrow_counts().get_count("source.selected"));
        assert_eq!(
            4,
            report.borrow_counts().get_count("skipped.excluded.files")
        );
        assert_eq!(
            1,
            report
                .borrow_counts()
                .get_count("skipped.excluded.directories",)
        );
    }

    #[cfg(unix)]
    #[test]
    fn symlinks() {
        let tf = TreeFixture::new();
        tf.create_symlink("from", "to");
        let report = Report::new();

        let lt = LiveTree::open(tf.path(), &report).unwrap();
        let result = lt.iter_entries(&report).unwrap().collect::<Vec<_>>();

        assert_eq!(&result[0].apath, "/");
        assert_eq!(&result[1].apath, "/from");
    }

    #[cfg(unix)]
    #[test]
    fn mtime_before_epoch() {
        // Like in <https://github.com/sourcefrog/conserve/issues/100>.
        let tf = TreeFixture::new();
        let file_path = tf.create_file("old_file");

        // We can't use the utime crate here because it passes the time as a u64,
        // even though the system's time_t is typically signed. libc doesn't have
        // this problem.
        let utimbuf = libc::utimbuf {
            actime: -36000,
            modtime: -36000,
        };
        let c_path = std::ffi::CString::new(file_path.to_str().unwrap().as_bytes()).unwrap();
        unsafe {
            // TODO: Move this to a patch to utime, or a fork, or local reimplementation.
            assert_eq!(libc::utime(c_path.as_ptr(), &utimbuf), 0);
        }

        // Just for confirmation when debugging that it's actually in 1969.
        std::process::Command::new("ls")
            .arg("-l")
            .arg(&tf.path())
            .spawn()
            .unwrap()
            .wait()
            .unwrap();

        let report = Report::new();
        let lt = LiveTree::open(tf.path(), &report).unwrap();
        let entries = lt.iter_entries(&report).unwrap().collect::<Vec<_>>();

        assert_eq!(&entries[0].apath, "/");
        assert_eq!(&entries[1].apath, "/old_file");
        dbg!(&entries[1].mtime());

        // This conversion panics in https://github.com/sourcefrog/conserve/issues/100 because it
        // produces a negative Duration.
        let _index_entry = IndexEntry::metadata_from(&entries[1]);
    }
}
