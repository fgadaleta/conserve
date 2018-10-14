// Conserve backup system.
// Copyright 2016, 2017, 2018 Martin Pool.

/// Utilities to set up test environments.
///
/// Fixtures that create directories will be automatically deleted when the object
/// is deleted.
use std::fs;
use std::io::Write;
use std::ops::Deref;
use std::path::{Path, PathBuf};

use tempdir;

use super::*;

/// A temporary archive, deleted when it goes out of scope.
///
/// The ScratchArchive can be treated as an Archive.
pub struct ScratchArchive {
    _tempdir: tempdir::TempDir, // held only for cleanup
    archive: Archive,
}

impl ScratchArchive {
    pub fn new() -> ScratchArchive {
        let tempdir = tempdir::TempDir::new("conserve_ScratchArchive").unwrap();
        let arch_dir = tempdir.path().join("archive");
        let archive = Archive::create(&arch_dir).unwrap();
        ScratchArchive {
            _tempdir: tempdir,
            archive,
        }
    }

    pub fn path(&self) -> &Path {
        self.archive.path()
    }

    #[allow(unused)]
    pub fn archive_dir_str(&self) -> &str {
        self.archive.path().to_str().unwrap()
    }

    pub fn setup_incomplete_empty_band(&self) {
        Band::create(&self.archive).unwrap();
    }

    pub fn store_two_versions(&self) {
        let srcdir = TreeFixture::new();
        srcdir.create_file("hello");
        srcdir.create_dir("subdir");
        srcdir.create_file("subdir/subfile");
        if SYMLINKS_SUPPORTED {
            srcdir.create_symlink("link", "target");
        }

        let report = Report::new();
        let lt = LiveTree::open(srcdir.path(), &report).unwrap();
        copy_tree(&lt, &mut BackupWriter::begin(&self).unwrap()).unwrap();

        srcdir.create_file("hello2");
        copy_tree(&lt, &mut BackupWriter::begin(&self).unwrap()).unwrap();
    }
}

impl Deref for ScratchArchive {
    type Target = Archive;

    /// ScratchArchive can be directly used as an archive.
    fn deref(&self) -> &Archive {
        &self.archive
    }
}

impl Default for ScratchArchive {
    fn default() -> Self {
        Self::new()
    }
}

/// A temporary tree for running a test.
///
/// Created in a temporary directory and automatically disposed when done.
pub struct TreeFixture {
    pub root: PathBuf,
    _tempdir: tempdir::TempDir, // held only for cleanup
}

impl TreeFixture {
    pub fn new() -> TreeFixture {
        let tempdir = tempdir::TempDir::new("conserve_TreeFixture").unwrap();
        let root = tempdir.path().to_path_buf();
        TreeFixture {
            _tempdir: tempdir,
            root,
        }
    }

    pub fn path(&self) -> &Path {
        &self.root
    }

    pub fn create_file(&self, relative_path: &str) {
        self.create_file_with_contents(relative_path, b"contents");
    }

    pub fn create_file_with_contents(&self, relative_path: &str, contents: &[u8]) {
        let full_path = self.root.join(relative_path);
        let mut f = fs::File::create(&full_path).unwrap();
        f.write_all(contents).unwrap();
    }

    pub fn create_dir(&self, relative_path: &str) {
        fs::create_dir(self.root.join(relative_path)).unwrap();
    }

    #[cfg(unix)]
    pub fn create_symlink(&self, relative_path: &str, target: &str) {
        use std::os::unix::fs as unix_fs;

        unix_fs::symlink(target, self.root.join(relative_path)).unwrap();
    }

    /// Symlinks are just not present on Windows.
    #[cfg(windows)]
    pub fn create_symlink(&self, _relative_path: &str, _target: &str) {}

    pub fn live_tree(&self) -> LiveTree {
        // TODO: Maybe allow deref TreeFixture to LiveTree.
        LiveTree::open(self.path(), &Report::new()).unwrap()
    }
}

impl Default for TreeFixture {
    fn default() -> Self {
        Self::new()
    }
}

/// List a directory.
///
/// Returns a list of filenames and a list of directory names respectively, forced to UTF-8, and
/// sorted naively as UTF-8.
#[cfg(test)]
pub fn list_dir(path: &Path) -> Result<(Vec<String>, Vec<String>)> {
    let mut file_names = Vec::<String>::new();
    let mut dir_names = Vec::<String>::new();
    for entry in fs::read_dir(path)? {
        let entry = entry.unwrap();
        let entry_filename = entry.file_name().into_string().unwrap();
        let entry_type = entry.file_type()?;
        if entry_type.is_file() {
            file_names.push(entry_filename);
        } else if entry_type.is_dir() {
            dir_names.push(entry_filename);
        } else {
            panic!("don't recognize file type of {:?}", entry_filename);
        }
    }
    file_names.sort_unstable();
    dir_names.sort_unstable();
    Ok((file_names, dir_names))
}

