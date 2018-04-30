use handlebars::*;
use itertools::join;
use std::collections::BTreeMap;
use serde_json::value::Value as Json;
use serde_json;
use url::Url;

// Change an array of items into a comma seperated list with formatting
// Usage: {{#comma-list array}}{{elementAttribute}}:{{attribute2}}{{/comma-list}}
pub(crate) fn comma_delimited_list_helper(
    h: &Helper,
    r: &Handlebars,
    rc: &mut RenderContext,
) -> HelperResult {
    let value = h.param(0)
        .ok_or_else(|| RenderError::new("Param not found for helper \"comma-list\""))?;

    match h.template() {
        Some(template) => match *value.value() {
            Json::Array(ref list) => {
                let len = list.len();

                let mut render_list = Vec::new();

                for i in 0..len {
                    let mut local_rc = rc.derive();

                    if let Some(inner_path) = value.path() {
                        let new_path = format!("{}/{}/[{}]", local_rc.get_path(), inner_path, i);
                        local_rc.set_path(new_path.clone());
                    }

                    if let Some(block_param) = h.block_param() {
                        let mut map = BTreeMap::new();
                        map.insert(block_param.to_string(), to_json(&list[i]));
                        local_rc.push_block_context(&map)?;
                    }

                    render_list.push(template.renders(r, &mut local_rc)?);
                }

                rc.writer.write_all(&join(&render_list, ",").into_bytes())?;

                Ok(())
            }
            Json::Null => Ok(()),
            _ => Err(RenderError::new(format!(
                "Param type is not array for helper \"comma-list\": {:?}",
                value
            ))),
        },
        None => Ok(()),
    }
}

pub(crate) fn equal_helper(h: &Helper, r: &Handlebars, rc: &mut RenderContext) -> HelperResult {
    let lvalue = h.param(0)
        .ok_or_else(|| RenderError::new("Left param not found for helper \"equal\""))?
        .value();
    let rvalue = h.param(1)
        .ok_or_else(|| RenderError::new("Right param not found for helper \"equal\""))?
        .value();

    let template = if lvalue == rvalue {
        h.template()
    } else {
        h.inverse()
    };

    match template {
        Some(ref t) => t.render(r, rc),
        None => Ok(()),
    }
}

// Escapes strings to that they can be safely used inside yaml (And JSON for that matter).
pub(crate) fn yaml_string_helper(
    h: &Helper,
    _: &Handlebars,
    rc: &mut RenderContext,
) -> HelperResult {
    let value = h.param(0)
        .ok_or_else(|| RenderError::new("Param not found for helper \"yaml-string\""))?;

    match *value.value() {
        ref s @ Json::String(_) => {
            let mut stringified = serde_json::to_string(&s).unwrap();
            if stringified.starts_with('"') {
                stringified.remove(0);
            }

            if stringified.ends_with('"') {
                stringified.pop();
            }

            rc.writer.write(stringified.as_bytes())?;

            Ok(())
        }
        Json::Null => Ok(()),
        _ => Err(RenderError::new(format!(
            "Param type is not string for helper \"yaml-string\": {:?}",
            value,
        ))),
    }
}

// Removes the trailing slash on an endpoint
pub(crate) fn url_rm_slash_helper(
    h: &Helper,
    _: &Handlebars,
    rc: &mut RenderContext,
) -> HelperResult {
    let value = h.param(0)
        .ok_or_else(|| RenderError::new("Param not found for helper \"url-rm-slash\""))?;

    match *value.value() {
        Json::String(ref s) => {
            if s.ends_with("/") {
                rc.writer.write(s[..s.len() - 1].as_bytes())?;
            } else {
                rc.writer.write(s.as_bytes())?;
            }

            Ok(())
        }
        Json::Null => Ok(()),
        _ => Err(RenderError::new(format!(
            "Param type is not string for helper \"url-rm-slash\": {:?}",
            value,
        ))),
    }
}

// Adds the trailing slashes on an endpoint
pub(crate) fn url_add_slash_helper(
    h: &Helper,
    _: &Handlebars,
    rc: &mut RenderContext,
) -> HelperResult {
    let value = h.param(0)
        .ok_or_else(|| RenderError::new("Param not found for helper \"url-add-slash\""))?;

    match *value.value() {
        Json::String(ref s) => {
            let output = if Url::parse(s).is_ok() && !s.ends_with("/") {
                format!("{}/", s)
            } else {
                s.clone()
            };

            rc.writer.write(output.as_bytes())?;

            Ok(())
        }
        Json::Null => Ok(()),
        _ => Err(RenderError::new(format!(
            "Param type is not string for helper \"url-add-slash\": {:?}",
            value,
        ))),
    }
}

// Removes the last slash plus content to the end of the string
pub(crate) fn url_rm_path(h: &Helper, _: &Handlebars, rc: &mut RenderContext) -> HelperResult {
    let value = h.param(0)
        .ok_or_else(|| RenderError::new("Param not found for helper \"url-rm-path\""))?;

    match *value.value() {
        Json::String(ref s) => {
            let url = if s.ends_with("/") {
                &s[..s.len() - 1]
            } else {
                &s
            };

            match Url::parse(url) {
                Ok(ref mut url) => {
                    if let Ok(ref mut paths) = url.path_segments_mut() {
                        paths.pop();
                    }

                    let mut url_str = url.as_str();
                    if url_str.ends_with("/") {
                        url_str = &url_str[..url_str.len() - 1];
                    }

                    rc.writer.write(url_str.as_bytes())?;

                    Ok(())
                }
                _ => {
                    rc.writer.write(s.as_bytes())?;
                    Ok(())
                }
            }
        }
        Json::Null => Ok(()),
        _ => Err(RenderError::new(format!(
            "Param type is not string for helper \"url-rm-path\": {:?}",
            value
        ))),
    }
}

pub(crate) fn lowercase_string_helper(
    h: &Helper,
    _: &Handlebars,
    rc: &mut RenderContext,
) -> HelperResult {
    let value = h.param(0)
        .ok_or_else(|| RenderError::new("Param not found for helper \"lowercase\""))?;

    match *value.value() {
        Json::String(ref s) => {
            rc.writer.write(s.to_lowercase().as_bytes())?;
            Ok(())
        }
        Json::Null => Ok(()),
        _ => Err(RenderError::new(format!(
            "Param type is not string for helper \"lowercase\": {:?}",
            value
        ))),
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use serde_json::{self, Value};

    fn config_fixture() -> Value {
        let mut config: Value = serde_json::from_str(&include_str!(
            "../../tests/fixtures/configs/config.TEST.json"
        )).unwrap();
        config["ConfigData"].take()
    }

    fn test_against_configs(handlebars: &Handlebars, template: &str, expected: &str) {
        let config_rendered = handlebars.render_template(template, &config_fixture());
        assert!(config_rendered.is_ok());
        assert_eq!(&config_rendered.unwrap(), expected);

        let null_rendered = handlebars.render_template(template, &Json::Null);
        assert!(null_rendered.is_ok());
        assert_eq!(&null_rendered.unwrap(), "");
    }

    #[test]
    fn test_comma_list() {
        let mut handlebars = Handlebars::new();
        handlebars.register_helper("comma-list", Box::new(comma_delimited_list_helper));

        test_against_configs(
            &handlebars,
            "{{#comma-list Memcache.Servers}}{{Endpoint}}:{{Port}}{{/comma-list}}",
            "192.168.1.100:1122,192.168.1.101:1122,192.168.1.102:1122",
        );
    }

    #[test]
    fn test_equal() {
        let mut handlebars = Handlebars::new();
        handlebars.register_helper("equal", Box::new(equal_helper));

        let templates = vec![
            (r#"{{#equal Region.Key "TEST"}}Foo{{/equal}}"#, "Foo"),
            (r#"{{#equal Region.Key null}}{{else}}Bar{{/equal}}"#, "Bar"),
        ];

        for (template, expected) in templates {
            test_against_configs(&handlebars, template, expected)
        }
    }

    #[test]
    fn test_yaml_string() {
        let mut handlebars = Handlebars::new();
        handlebars.register_helper("yaml-string", Box::new(yaml_string_helper));

        test_against_configs(
            &handlebars,
            "{{yaml-string DB.Endpoint}}",
            r#"host-name\\TEST\""#,
        );
    }

    #[test]
    fn test_url_rm_slash() {
        let mut handlebars = Handlebars::new();
        handlebars.register_helper("url-rm-slash", Box::new(url_rm_slash_helper));

        test_against_configs(
            &handlebars,
            "{{url-rm-slash SlashService.endpoint}}",
            "https://slash.com",
        );
    }

    #[test]
    fn test_url_add_slash() {
        let mut handlebars = Handlebars::new();
        handlebars.register_helper("url-add-slash", Box::new(url_add_slash_helper));

        let templates = vec![
            (
                "{{url-add-slash NonSlashService.endpoint}}",
                "https://nonslash.com/",
            ),
            (
                "{{url-add-slash NonSlashService.notAnEndpoint}}",
                "no-protocol.no-slash.com",
            ),
        ];

        for (template, expected) in templates {
            test_against_configs(&handlebars, template, expected)
        }
    }

    #[test]
    fn test_url_rm_path() {
        let mut handlebars = Handlebars::new();
        handlebars.register_helper("url-rm-path", Box::new(url_rm_path));

        let templates = vec![
            (
                "{{url-rm-path PathService.endpoint}}",
                "https://path.com/path",
            ),
            (
                "{{url-rm-path PathService.trailingSlash}}",
                "https://trailing-path.com/path",
            ),
        ];

        for (template, expected) in templates {
            test_against_configs(&handlebars, template, expected)
        }
    }

    #[test]
    fn test_double_url_rm_path() {
        let mut handlebars = Handlebars::new();
        handlebars.register_helper("url-rm-path", Box::new(url_rm_path));

        test_against_configs(
            &handlebars,
            "{{url-rm-path (url-rm-path PathService.trailingSlash)}}",
            "https://trailing-path.com",
        );
    }

    #[test]
    fn test_lowercase() {
        let mut handlebars = Handlebars::new();
        handlebars.register_helper("lowercase", Box::new(lowercase_string_helper));

        test_against_configs(&handlebars, "{{lowercase UpperCaseString}}", "uppercase");
    }
}
