use handlebars::*;
use serde_json::value::Value as Json;
use url::Url;

#[derive(Clone, Copy)]
pub struct UrlAddSlashHelper;

impl HelperDef for UrlAddSlashHelper {
    // Adds the trailing slashes on an endpoint
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
            .ok_or_else(|| RenderError::new("Param not found for helper \"url-add-slash\""))?;

        match *value.value() {
            Json::String(ref s) => {
                let output = if Url::parse(s).is_ok() && !s.ends_with("/") {
                    format!("{}/", s)
                } else {
                    s.clone()
                };

                out.write(&output)?;

                Ok(())
            }
            Json::Null => Ok(()),
            _ => Err(RenderError::new(format!(
                "Param type is not string for helper \"url-add-slash\": {:?}",
                value,
            ))),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use transform::test::test_against_configs;

    #[test]
    fn test_url_add_slash() {
        let mut handlebars = Handlebars::new();
        handlebars.register_helper("url-add-slash", Box::new(UrlAddSlashHelper));

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
}
