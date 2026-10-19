//! SFTP domain types (docs §9.2).
//! Author: Passwd Word

use serde::{Deserialize, Serialize};

/// Kind of a remote file entry (docs §9.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileKind {
    File,
    Directory,
    Symlink,
    Other,
}

/// A remote directory entry (docs §9.2).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub kind: FileKind,
    pub size: u64,
    pub modified_at: Option<i64>,
    pub permissions: Option<String>,
}

impl FileEntry {
    /// Whether the entry can be descended into.
    pub fn is_dir(&self) -> bool {
        self.kind == FileKind::Directory
    }
}

/// Sort key for directory listings: directories first, then names.
pub fn sort_entries(entries: &mut [FileEntry]) {
    entries.sort_by(|a, b| {
        (b.is_dir())
            .cmp(&a.is_dir())
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dirs_sort_first() {
        let mut entries = vec![
            FileEntry { name: "b.txt".into(), path: "/b.txt".into(), kind: FileKind::File, size: 1, modified_at: None, permissions: None },
            FileEntry { name: "adir".into(), path: "/adir".into(), kind: FileKind::Directory, size: 0, modified_at: None, permissions: None },
            FileEntry { name: "a.txt".into(), path: "/a.txt".into(), kind: FileKind::File, size: 1, modified_at: None, permissions: None },
        ];
        sort_entries(&mut entries);
        assert_eq!(entries[0].name, "adir");
        assert_eq!(entries[1].name, "a.txt");
        assert_eq!(entries[2].name, "b.txt");
    }
}
