use handlebars::*;

#[derive(Clone, Copy)]
pub struct EqualHelper;

impl HelperDef for EqualHelper {
    fn call<'reg: 'rc, 'rc, 'ctx>(
        &self,
        h: &Helper<'reg, 'rc>,
        r: &'reg Handlebars,
        ctx: &'ctx Context,
        rc: &mut RenderContext<'reg, 'ctx>,
        out: &mut dyn Output,
    ) -> HelperResult {
        let lvalue = h
            .param(0)
            .ok_or_else(|| RenderError::new("Left param not found for helper \"equal\""))?
            .value();
        let rvalue = h
            .param(1)
            .ok_or_else(|| RenderError::new("Right param not found for helper \"equal\""))?
            .value();

        let comparison = lvalue == rvalue;

        if h.is_block() {
            let template = if comparison {
                h.template()
            } else {
                h.inverse()
            };

            match template {
                Some(t) => t.render(r, ctx, rc, out),
                None => Ok(()),
            }
        } else {
            if comparison {
                out.write(&comparison.to_string())?;
            }

            Ok(())
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::transform::test::test_against_configs;

    #[test]
    fn test_equal() {
        let mut handlebars = Handlebars::new();
        handlebars.register_helper("equal", Box::new(EqualHelper));
        handlebars.register_helper("eq", Box::new(EqualHelper));

        let templates = vec![
            (r#"{{#equal Region.Key "TEST"}}Foo{{/equal}}"#, "Foo"),
            (r#"{{#equal Region.Key null}}{{else}}Bar{{/equal}}"#, "Bar"),
            (r#"{{#eq Region.Key "TEST"}}Foo{{/eq}}"#, "Foo"),
            (r#"{{#eq Region.Key null}}{{else}}Bar{{/eq}}"#, "Bar"),
            (r#"{{#if (equal Region.Key "TEST")}}Foo{{/if}}"#, "Foo"),
            (
                r#"{{#if (equal Region.Key null)}}{{else}}Bar{{/if}}"#,
                "Bar",
            ),
            (r#"{{#if (eq Region.Key "TEST")}}Foo{{/if}}"#, "Foo"),
            (r#"{{#if (eq Region.Key null)}}{{else}}Bar{{/if}}"#, "Bar"),
        ];

        for (template, expected) in templates {
            test_against_configs(&handlebars, template, expected)
        }
    }
}
