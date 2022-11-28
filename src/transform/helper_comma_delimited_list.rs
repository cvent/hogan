use handlebars::*;
use itertools::join;
use serde_json::value::Value as Json;

#[derive(Clone, Copy)]
pub struct CommaDelimitedListHelper;

impl HelperDef for CommaDelimitedListHelper {
    // Change an array of items into a comma seperated list with formatting
    // Usage: {{#comma-list array}}{{elementAttribute}}:{{attribute2}}{{/comma-list}}
    fn call<'reg: 'rc, 'rc, 'ctx>(
        &self,
        h: &Helper<'reg, 'rc>,
        r: &'reg Handlebars,
        ctx: &'ctx Context,
        rc: &mut RenderContext<'reg, 'ctx>,
        out: &mut dyn Output,
    ) -> HelperResult {
        let value = h
            .param(0)
            .ok_or_else(|| RenderError::new("Param not found for helper \"comma-list\""))?;

        match h.template() {
            Some(template) => match *value.value() {
                Json::Array(ref list) => {
                    let mut render_list = Vec::new();

                    for (i, item) in list.iter().enumerate() {
                        let mut local_rc = rc.clone();
                        let block_rc = local_rc.block_mut().unwrap();
                        if let Some(inner_path) = value.context_path() {
                            let block_path = block_rc.base_path_mut();
                            block_path.append(&mut inner_path.to_owned());
                            block_path.push(i.to_string());
                        }

                        if let Some(block_param) = h.block_param() {
                            let mut new_block = BlockContext::new();
                            let mut block_params = BlockParams::new();
                            block_params.add_value(block_param, to_json(item))?;
                            new_block.set_block_params(block_params);
                            local_rc.push_block(new_block);

                            render_list.push(template.renders(r, ctx, &mut local_rc)?);
                        } else {
                            render_list.push(template.renders(r, ctx, &mut local_rc)?);
                        }
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
