use handlebars::*;
use itertools::join;
use serde_json::value::Value as Json;
use std::collections::BTreeMap;

#[derive(Clone, Copy)]
pub struct CommaDelimitedListHelper;

impl HelperDef for CommaDelimitedListHelper {
    // Change an array of items into a comma seperated list with formatting
    // Usage: {{#comma-list array}}{{elementAttribute}}:{{attribute2}}{{/comma-list}}
    fn call<'reg: 'rc, 'rc>(
        &self,
        h: &Helper<'reg, 'rc>,
        r: &'reg Handlebars,
        ctx: &Context,
        rc: &mut RenderContext<'reg>,
        out: &mut Output,
    ) -> HelperResult {
        let value = h
            .param(0)
            .ok_or_else(|| RenderError::new("Param not found for helper \"comma-list\""))?;

        match h.template() {
            Some(template) => match *value.value() {
                Json::Array(ref list) => {
                    let len = list.len();

                    let mut render_list = Vec::new();

                    for (i, item) in list.iter().enumerate().take(len) {
                        let mut local_rc = rc.derive();

                        if let Some(inner_path) = value.path() {
                            let new_path =
                                format!("{}/{}/[{}]", local_rc.get_path(), inner_path, i);
                            local_rc.set_path(new_path.clone());
                        }

                        if let Some(block_param) = h.block_param() {
                            let mut map = BTreeMap::new();
                            map.insert(block_param.to_string(), to_json(item));
                            local_rc.push_block_context(&map)?;
                        }

                        render_list.push(template.renders(r, ctx, &mut local_rc)?);
                    }

                    out.write(&join(&render_list, ","))?;

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
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::transform::test::test_against_configs;

    #[test]
    fn test_comma_list() {
        let mut handlebars = Handlebars::new();
        handlebars.register_helper("comma-list", Box::new(CommaDelimitedListHelper));

        test_against_configs(
            &handlebars,
            "{{#comma-list Memcache.Servers}}{{Endpoint}}:{{Port}}{{/comma-list}}",
            "192.168.1.100:1122,192.168.1.101:1122,192.168.1.102:1122",
        );
    }
}
