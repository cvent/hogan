use config::Environment;
use failure::Error;
use find_file_paths;
use handlebars::Handlebars;
use regex::Regex;
use std::fs::File;
use std::io::{Cursor, Read, Write};
use std::path::PathBuf;
use zip::CompressionMethod::Stored;
use zip::write::{FileOptions, ZipWriter};

pub struct TemplateDir {
    directory: PathBuf,
}

impl TemplateDir {
    pub fn new(path: PathBuf) -> Result<TemplateDir, Error> {
        if !path.is_dir() {
            bail!(
                "{:?} either does not exist or is not a directory. It needs to be both",
                path
            )
        } else {
            Ok(TemplateDir { directory: path })
        }
    }

    pub fn find(&self, filter: Regex) -> Vec<Template<File>> {
        find_file_paths(&self.directory, filter)
            .filter_map(|path| Template::from_path_buf(path).ok())
            .collect()
    }
}

pub struct Template<R: Read> {
    pub path: PathBuf,
    pub read: R,
}

impl Template<File> {
    fn from_path_buf(path: PathBuf) -> Result<Template<File>, Error> {
        Ok(Template {
            path: path.clone(),
            read: File::open(path)?,
        })
    }
}

impl<R: Read> Template<R> {
    pub fn render(
        &mut self,
        handlebars: &Handlebars,
        environment: &Environment,
    ) -> Result<Rendered, Error> {
        let mut buf = Cursor::new(Vec::new());
        handlebars.render_template_source_to_write(
            &mut self.read,
            &environment.config_data,
            &mut buf,
        )?;

        Ok(Rendered {
            path: self.path.with_file_name(
                self.path
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .replace("template", &environment.environment),
            ),
            contents: buf.into_inner(),
        })
    }

    pub fn render_to_zip(
        &mut self,
        handlebars: &Handlebars,
        environments: &Vec<Environment>,
    ) -> Result<Vec<u8>, Error> {
        let options = FileOptions::default().compression_method(Stored);
        let mut zip = ZipWriter::new(Cursor::new(Vec::new()));

        for environment in environments {
            let rendered = self.render(&handlebars, &environment)?;
            zip.start_file(
                rendered.path.file_name().unwrap().to_string_lossy(),
                options,
            )?;
            zip.write_all(&rendered.contents)?;
        }

        Ok(zip.finish()?.into_inner())
    }
}

pub struct Rendered {
    pub path: PathBuf,
    pub contents: Vec<u8>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use regex::RegexBuilder;

    #[test]
    fn test_find_all_templates() {
        let template_dir = TemplateDir::new(PathBuf::from("tests/fixtures/Projects")).unwrap();
        let templates = template_dir.find(
            RegexBuilder::new("(.*\\.)?template(\\.Release|\\-liquibase|\\-quartz)?\\.([Cc]onfig|yaml|properties)$")
                .case_insensitive(true)
                .build()
                .unwrap(),
        );
        assert_eq!(templates.len(), 3)
    }
}
