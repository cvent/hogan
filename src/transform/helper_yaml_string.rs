use handlebars::*;
use serde_json::value::Value as Json;

#[derive(Clone, Copy)]
pub struct YamlStringHelper;

impl HelperDef for YamlStringHelper {
    // Escapes strings to that they can be safely used inside yaml (And JSON for that matter).
    fn call<'reg: 'rc, 'rc, 'ctx>(
        &self,
        h: &Helper<'reg, 'rc>,
        _: &'reg Handlebars,
        _: &'ctx Context,
        _: &mut RenderContext<'reg, 'ctx>,
        out: &mut dyn Output,
    ) -> HelperResult {
        let value = h
            .param(0)
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

                out.write(&stringified)?;

                Ok(())
            }
            Json::Null => Ok(()),
            _ => Err(RenderError::new(format!(
                "Param type is not string for helper \"yaml-string\": {:?}",
                value,
            ))),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::transform::test::test_against_configs;

    #[test]
    fn test_yaml_string() {
        let mut handlebars = Handlebars::new();
        handlebars.register_helper("yaml-string", Box::new(YamlStringHelper));

        test_against_configs(
            &handlebars,
            "{{yaml-string DB.Endpoint}}",
            r#"host-name\\TEST\""#,
        );
    }
}
