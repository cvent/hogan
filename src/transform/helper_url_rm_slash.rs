use handlebars::*;
use serde_json::value::Value as Json;

#[derive(Clone, Copy)]
pub struct UrlRmSlashHelper;

impl HelperDef for UrlRmSlashHelper {
    // Removes the trailing slash on an endpoint
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
            .ok_or_else(|| RenderError::new("Param not found for helper \"url-rm-slash\""))?;

        match *value.value() {
            Json::String(ref s) => {
                if s.ends_with("/") {
                    out.write(&s[..s.len() - 1])?;
                } else {
                    out.write(s)?;
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
}

#[cfg(test)]
mod test {
    use super::*;
    use transform::test::test_against_configs;

    #[test]
    fn test_url_rm_slash() {
        let mut handlebars = Handlebars::new();
        handlebars.register_helper("url-rm-slash", Box::new(UrlRmSlashHelper));

        test_against_configs(
            &handlebars,
            "{{url-rm-slash SlashService.endpoint}}",
            "https://slash.com",
        );
    }
}
