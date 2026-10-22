// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::fs::File;
use std::io::Read;
use std::path::Path;

/// Largest filter list the host will read, in bytes.
pub const MAX_FILTER_LIST_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug)]
pub enum ListError {
    Io {
        path: std::path::PathBuf,
        source: std::io::Error,
    },
    Utf8 {
        path: std::path::PathBuf,
    },
    TooLarge {
        path: std::path::PathBuf,
        limit: usize,
    },
}

impl std::fmt::Display for ListError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io { path, source } => {
                write!(
                    formatter,
                    "failed to read filter list {}: {source}",
                    path.display()
                )
            }
            Self::Utf8 { path } => {
                write!(
                    formatter,
                    "filter list {} is not valid UTF-8",
                    path.display()
                )
            }
            Self::TooLarge { path, limit } => write!(
                formatter,
                "filter list {} exceeds {limit} bytes",
                path.display()
            ),
        }
    }
}

impl std::error::Error for ListError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Utf8 { .. } | Self::TooLarge { .. } => None,
        }
    }
}

pub fn read_filter_list(path: &Path) -> Result<String, ListError> {
    let file = File::open(path).map_err(|source| ListError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let mut reader = file.take(MAX_FILTER_LIST_BYTES as u64 + 1);
    let mut bytes = Vec::new();
    reader
        .read_to_end(&mut bytes)
        .map_err(|source| ListError::Io {
            path: path.to_path_buf(),
            source,
        })?;
    if bytes.len() > MAX_FILTER_LIST_BYTES {
        return Err(ListError::TooLarge {
            path: path.to_path_buf(),
            limit: MAX_FILTER_LIST_BYTES,
        });
    }
    String::from_utf8(bytes).map_err(|_| ListError::Utf8 {
        path: path.to_path_buf(),
    })
}

pub fn read_filter_lists<P: AsRef<Path>>(paths: &[P]) -> Result<Vec<String>, ListError> {
    paths
        .iter()
        .map(|path| read_filter_list(path.as_ref()))
        .collect()
}
