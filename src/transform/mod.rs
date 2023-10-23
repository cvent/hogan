use handlebars::Handlebars;

mod helper_comma_delimited_list;
mod helper_equal;
mod helper_lowercase;
mod helper_or;
mod helper_url_add_slash;
mod helper_url_rm_path;
mod helper_url_rm_slash;
mod helper_yaml_string;

use self::helper_comma_delimited_list::CommaDelimitedListHelper;
use self::helper_equal::EqualHelper;
use self::helper_lowercase::LowercaseHelper;
use self::helper_or::OrHelper;
use self::helper_url_add_slash::UrlAddSlashHelper;
use self::helper_url_rm_path::UrlRmPathHelper;
use self::helper_url_rm_slash::UrlRmSlashHelper;
use self::helper_yaml_string::YamlStringHelper;

//This fn was changed here https://github.com/sunng87/handlebars-rust/pull/366 which added additional characters to the list
//To maintain backwards compatibility we are reverting to the original default escape fn
pub fn old_escape_html(s: &str) -> String {
    let mut output = String::new();

    for c in s.chars() {
        match c {
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' => output.push_str("&quot;"),
            '&' => output.push_str("&amp;"),
            _ => output.push(c),
        }
    }
    output
}

pub fn handlebars<'a>(strict: bool) -> Handlebars<'a> {
    let mut handlebars = Handlebars::new();
    handlebars.set_strict_mode(strict);
    handlebars.register_helper("comma-list", Box::new(CommaDelimitedListHelper));
    handlebars.register_helper("equal", Box::new(EqualHelper));
    handlebars.register_helper("eq", Box::new(EqualHelper));
    handlebars.register_helper("lowercase", Box::new(LowercaseHelper));
    handlebars.register_helper("or", Box::new(OrHelper));
    handlebars.register_helper("url-add-slash", Box::new(UrlAddSlashHelper));
    handlebars.register_helper("url-rm-path", Box::new(UrlRmPathHelper));
    handlebars.register_helper("url-rm-slash", Box::new(UrlRmSlashHelper));
    handlebars.register_helper("yaml-string", Box::new(YamlStringHelper));
    handlebars.register_escape_fn(old_escape_html);
    handlebars
}

#[cfg(test)]
mod test {

    use super::*;
    use serde_json::{self, Value};

    fn config_fixture() -> Value {
        let mut config: Value = serde_json::from_str(&include_str!(
            "../../tests/fixtures/configs/config.TEST.json"
        ))
        .unwrap();
        config["ConfigData"].take()
    }

    pub(crate) fn test_against_configs(handlebars: &Handlebars, template: &str, expected: &str) {
        let config_rendered = handlebars.render_template(template, &config_fixture());
        assert!(config_rendered.is_ok());
        assert_eq!(&config_rendered.unwrap(), expected);

        let null_rendered = handlebars.render_template(template, &Value::Null);
        assert!(null_rendered.is_ok());
        assert_eq!(&null_rendered.unwrap(), "");
    }

    pub(crate) fn test_error_against_configs(
        handlebars: &Handlebars,
        template: &str,
        expected: &str,
    ) {
        let config_rendered = handlebars.render_template(template, &config_fixture());
        assert!(!config_rendered.is_ok());
        assert!(&config_rendered.unwrap_err().to_string().ends_with(expected));
    }
}
