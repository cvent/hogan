#![warn(unused)]

#[macro_use]
extern crate failure;
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

use config::Environment;
use failure::Error;
use handlebars::Handlebars;
use regex::Regex;
use std::fs::File;
use std::path::{Path, PathBuf};
use walkdir::{DirEntry, WalkDir};

pub mod config;
pub mod transform;
pub mod template;

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

pub fn generate_configs(
    handlebars: &mut Handlebars,
    environments: Vec<Environment>,
    template_paths: Vec<PathBuf>,
) -> Result<(), Error> {
    for template_path in &template_paths {
        handlebars.register_template_file(&template_path.to_string_lossy(), template_path)?;
    }

    for environment in environments {
        info!("Updating templates for {}", environment.environment);

        for template_path in &template_paths {
            let template_path = template_path.to_string_lossy();
            let path = template_path.replace("template", &environment.environment);
            let mut file = File::create(&path)?;

            debug!("Transforming {}", path);
            if let Err(e) =
                handlebars.render_to_write(&template_path, &environment.config_data, &mut file)
            {
                bail!("Error transforming {} due to {}", &path, e);
            }
        }
    }

    Ok(())
}
