use crate::app::config::App;
use crate::app::config::AppCommon;
use anyhow::{Context, Result};
use hogan::config::ConfigDir;
use hogan::error::HoganError;
use hogan::template::TemplateDir;
use regex::Regex;
use std::fs::File;
use std::fs::OpenOptions;
use std::io::ErrorKind::AlreadyExists;
use std::io::Write;
use std::path::PathBuf;

pub fn cli(
    templates_path: PathBuf,
    environments_regex: Regex,
    templates_regex: Regex,
    common: AppCommon,
    ignore_existing: bool,
) -> Result<()> {
    let handlebars = hogan::transform::handlebars(common.strict);

    let template_dir = TemplateDir::new(templates_path)?;
    let mut templates = template_dir.find(templates_regex);
    println!("Loaded {} template file(s)", templates.len());

    let config_dir = ConfigDir::new(common.configs_url, &common.ssh_key)?;
    let environments = config_dir.find(App::config_regex(&environments_regex)?);
    println!("Loaded {} config file(s)", environments.len());

    for environment in environments {
        println!("Updating templates for {}", environment.environment);

        for template in &mut templates {
            debug!("Transforming {:?}", template.path);

            let rendered = template.render(&handlebars, &environment)?;
            trace!("Rendered: {:?}", rendered.contents);

            if ignore_existing {
                if let Err(e) = match OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&rendered.path)
                {
                    Ok(ref mut f) => f.write_all(&rendered.contents),
                    Err(ref e) if e.kind() == AlreadyExists => {
                        println!("Skipping {:?} - config already exists.", rendered.path);
                        trace!("Skipping {:?} - config already exists.", rendered.path);
                        Ok(())
                    }
                    Err(e) => Err(e),
                } {
                    return Err(HoganError::UnknownError {
                        msg: format!("Error transforming {:?} due to {:?}", rendered.path, e),
                    })
                    .with_context(|| "Error while ignoring existing");
                }
            } else {
                File::create(&rendered.path)?
                    .write_all(&rendered.contents)
                    .with_context(|| format!("Error transforming {:?}", rendered.path))?;
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use assert_cmd;
    use dir_diff;
    use fs_extra;
    use predicates;
    use tempfile;

    use self::assert_cmd::prelude::*;
    use self::fs_extra::dir;
    use self::predicates::prelude::*;
    use std::io::Write;
    use std::path::Path;
    use std::process::Command;

    #[cfg(not(all(target_env = "msvc", target_arch = "x86_64")))]
    #[test]
    fn test_transform() {
        let temp_dir = tempfile::tempdir().unwrap();

        fs_extra::copy_items(
            &vec!["tests/fixtures/projects/templates"],
            temp_dir.path(),
            &dir::CopyOptions::new(),
        )
        .unwrap();

        let templates_path = temp_dir.path().join("templates");

        let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();

        let cmd = cmd.args(&[
            "transform",
            "--configs",
            "tests/fixtures/configs",
            "--templates",
            templates_path.to_str().unwrap(),
        ]);

        cmd.assert().success();

        cmd.assert().stdout(
            predicate::str::contains(format!(r#"Finding Files: {:?}"#, templates_path)).from_utf8(),
        );

        cmd.assert().stdout(
            predicate::str::contains(
                r"regex: /^[^.]*(\w+\.)*template([-.].+)?\.(config|ya?ml|properties)/",
            )
            .from_utf8(),
        );

        cmd.assert()
            .stdout(predicate::str::contains("Loaded 6 template file(s)").from_utf8());

        cmd.assert().stdout(
            predicate::str::contains(r#"Finding Files: "tests/fixtures/configs""#).from_utf8(),
        );

        cmd.assert()
            .stdout(predicate::str::contains(r#"regex: /config\..+\.json$/"#).from_utf8());

        cmd.assert()
            .stdout(predicate::str::contains("Loaded 4 config file(s)").from_utf8());

        for environment in ["EMPTY", "ENVTYPE", "TEST", "TEST2"].iter() {
            cmd.assert().stdout(
                predicate::str::contains(format!("Updating templates for {}", environment))
                    .from_utf8(),
            );
        }

        assert!(!dir_diff::is_different(
            &templates_path.join("project-1"),
            &Path::new("tests/fixtures/projects/rendered/project-1")
        )
        .unwrap());

        assert!(!dir_diff::is_different(
            &templates_path.join("project-2"),
            &Path::new("tests/fixtures/projects/rendered/project-2")
        )
        .unwrap());
    }

    #[cfg(not(all(target_env = "msvc", target_arch = "x86_64")))]
    #[test]
    fn test_ignore_existing() {
        let temp_dir = tempfile::tempdir().unwrap();

        fs_extra::copy_items(
            &vec!["tests/fixtures/projects/templates"],
            temp_dir.path(),
            &dir::CopyOptions::new(),
        )
        .unwrap();

        let templates_path = temp_dir.path().join("templates");

        let ignore_path = templates_path.join("project-1/Web.EMPTY.config");
        if let Ok(ref mut f) = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .open(&ignore_path)
        {
            f.write_all(b"Hamburger.")
                .expect("Failed to create test file for ignore.")
        }

        let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();
        let cmd = cmd.args(&[
            "transform",
            "--configs",
            "tests/fixtures/configs",
            "--templates",
            templates_path.to_str().unwrap(),
            "-i",
        ]);

        cmd.assert().success();

        // assert that running the command with the ignore flag
        // did not overwrite the manually created project-1/Web.EMPTY.config
        let data2 =
            std::fs::read_to_string(&ignore_path).expect("Failed to read test file for ignore.");
        assert!(data2 == "Hamburger.");

        // after running the command again without the ignore flag
        // assert that the configs now match those in the rendered directory
        let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();
        let cmd = cmd.args(&[
            "transform",
            "--configs",
            "tests/fixtures/configs",
            "--templates",
            templates_path.to_str().unwrap(),
        ]);
        cmd.assert().success();

        assert!(!dir_diff::is_different(
            &templates_path.join("project-1"),
            &Path::new("tests/fixtures/projects/rendered/project-1")
        )
        .unwrap());

        assert!(!dir_diff::is_different(
            &templates_path.join("project-2"),
            &Path::new("tests/fixtures/projects/rendered/project-2")
        )
        .unwrap());
    }
}
