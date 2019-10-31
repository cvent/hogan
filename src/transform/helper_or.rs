use handlebars::*;

#[derive(Clone, Copy)]
pub struct OrHelper;

impl HelperDef for OrHelper {
    fn call<'reg: 'rc, 'rc>(
        &self,
        h: &Helper<'reg, 'rc>,
        r: &'reg Handlebars,
        ctx: &Context,
        rc: &mut RenderContext<'reg>,
        out: &mut dyn Output,
    ) -> HelperResult {
        if h.params().len() < 2 {
            return Err(RenderError::new("'or' requires at least 2 parameters"));
        }

        let comparison = h
            .params()
            .iter()
            .any(|p| p.value().as_str().map_or(false, |v| !v.is_empty()));

        if h.is_block() {
            let template = if comparison {
                h.template()
            } else {
                h.inverse()
            };

            match template {
                Some(ref t) => t.render(r, ctx, rc, out),
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
    use crate::transform::helper_equal::EqualHelper;
    use crate::transform::test::test_against_configs;
    use crate::transform::test::test_error_against_configs;

    #[test]
    fn test_or() {
        let mut handlebars = Handlebars::new();
        handlebars.register_helper("eq", Box::new(EqualHelper));
        handlebars.register_helper("or", Box::new(OrHelper));

        let templates = vec![
            (
                r#"{{#or (eq Region.Key "TEST") (eq Region.Key "TEST2")}}Foo{{/or}}"#,
                "Foo",
            ),
            (
                r#"{{#or (eq Region.Key null) (eq Region.Key "NO")}}{{else}}Bar{{/or}}"#,
                "Bar",
            ),
            (
                r#"{{#if (or (eq Region.Key "TEST") (eq Region.Key "TEST2"))}}Foo{{/if}}"#,
                "Foo",
            ),
            (
                r#"{{#if (or (eq Region.Key null) (eq Region.Key "NO"))}}{{else}}Bar{{/if}}"#,
                "Bar",
            ),
            (
                r#"{{#or (eq Region.Key "NO") (eq Region.Key "TEST2") (eq Region.Key "TEST")}}Foo{{/or}}"#,
                "Foo",
            ),
        ];

        let error_templates = vec![(
            r#"{{#or (eq Region.Key "NO") }}Foo{{/or}}"#,
            "'or' requires at least 2 parameters",
        )];

        for (template, expected) in templates {
            test_against_configs(&handlebars, template, expected)
        }

        for (template, expected) in error_templates {
            test_error_against_configs(&handlebars, template, expected)
        }
    }
}
