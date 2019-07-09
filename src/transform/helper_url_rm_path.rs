use handlebars::*;
use serde_json::value::Value as Json;
use url::Url;

#[derive(Clone, Copy)]
pub struct UrlRmPathHelper;

impl HelperDef for UrlRmPathHelper {
    // Removes the last slash plus content to the end of the string
    fn call<'reg: 'rc, 'rc>(
        &self,
        h: &Helper<'reg, 'rc>,
        _: &'reg Handlebars,
        _: &Context,
        _: &mut RenderContext<'reg>,
        out: &mut Output,
    ) -> HelperResult {
        let value = h
            .param(0)
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

                        out.write(url_str)?;

                        Ok(())
                    }
                    _ => {
                        out.write(s)?;
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
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::transform::test::test_against_configs;

    #[test]
    fn test_url_rm_path() {
        let mut handlebars = Handlebars::new();
        handlebars.register_helper("url-rm-path", Box::new(UrlRmPathHelper));

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
        handlebars.register_helper("url-rm-path", Box::new(UrlRmPathHelper));

        test_against_configs(
            &handlebars,
            "{{url-rm-path (url-rm-path PathService.trailingSlash)}}",
            "https://trailing-path.com",
        );
    }
}
