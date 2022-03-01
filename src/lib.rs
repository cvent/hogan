#[macro_use]
extern crate log;
#[macro_use]
extern crate serde_derive;

pub mod config;
pub mod error;
pub mod git;
pub mod template;
pub mod transform;

use regex::Regex;
use std::path::{Path, PathBuf};
use walkdir::{DirEntry, WalkDir};

pub fn find_file_paths(path: &Path, filter: Regex) -> Box<dyn Iterator<Item = PathBuf>> {
    fn match_filter(entry: &DirEntry, filter: &Regex) -> bool {
        entry
            .file_name()
            .to_str()
            .map(|s| filter.is_match(s))
            .unwrap_or(false)
    }

    println!("Finding Files: {:?}", path);
    println!("regex: /{}/", filter);

    Box::new(
        WalkDir::new(path)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .filter(move |e| match_filter(e, &filter))
            .map(|e| e.path().to_path_buf()),
    )
}
