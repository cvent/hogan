#![warn(unused)]

extern crate handlebars;
extern crate itertools;
extern crate json_patch;
#[macro_use]
extern crate log;
extern crate regex;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;
extern crate url;
extern crate walkdir;

pub mod config;
pub mod transform;
pub mod template;

use std::path::{Path, PathBuf};

use regex::Regex;
use walkdir::{DirEntry, WalkDir};

pub fn find_file_paths(base_path: &Path, filter: Regex) -> Box<Iterator<Item = PathBuf>> {
    fn match_filter(entry: &DirEntry, filter: &Regex) -> bool {
        entry
            .file_name()
            .to_str()
            .map(|s| filter.is_match(&s))
            .unwrap_or(false)
    }

    info!("Finding Files: {:?}", base_path);
    info!("regex: /{}/", filter);

    Box::new(
        WalkDir::new(base_path)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .filter(move |e| match_filter(e, &filter))
            .map(|e| e.path().to_path_buf()),
    )
}
