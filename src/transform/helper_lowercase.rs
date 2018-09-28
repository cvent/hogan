use handlebars::*;
use serde_json::value::Value as Json;

#[derive(Clone, Copy)]
pub struct LowercaseHelper;

impl HelperDef for LowercaseHelper {
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
            .ok_or_else(|| RenderError::new("Param not found for helper \"lowercase\""))?;

        match *value.value() {
            Json::String(ref s) => {
                out.write(&s.to_lowercase())?;
                Ok(())
            }
            Json::Null => Ok(()),
            _ => Err(RenderError::new(format!(
                "Param type is not string for helper \"lowercase\": {:?}",
                value
            ))),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use transform::test::test_against_configs;

    #[test]
    fn test_lowercase() {
        let mut handlebars = Handlebars::new();
        handlebars.register_helper("lowercase", Box::new(LowercaseHelper));

        test_against_configs(&handlebars, "{{lowercase UpperCaseString}}", "uppercase");
    }

}
