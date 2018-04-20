use find_file_paths;
use regex::Regex;
use std::path::{Path, PathBuf};

pub fn templates(base_path: &Path, filter: Regex) -> Vec<PathBuf> {
    find_file_paths(base_path, filter).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use regex::RegexBuilder;

    #[test]
    fn test_find_all_templates() {
        let templates = templates(
            &PathBuf::from("tests/fixtures/Projects"),
            RegexBuilder::new("(.*\\.)?template(\\.Release|\\-liquibase|\\-quartz)?\\.([Cc]onfig|yaml|properties)$")
                .case_insensitive(true)
                .build()
                .unwrap(),
        );
        assert_eq!(templates.len(), 3)
    }
}
