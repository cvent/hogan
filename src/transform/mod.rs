use handlebars::Handlebars;

pub mod helpers;

pub fn handlebars(strict: bool) -> Handlebars {
    let mut handlebars = Handlebars::new();
    handlebars.set_strict_mode(strict);
    handlebars.register_helper("comma-list", Box::new(helpers::comma_delimited_list_helper));
    handlebars.register_helper("equal", Box::new(helpers::equal_helper));
    handlebars.register_helper("eq", Box::new(helpers::equal_helper));
    handlebars.register_helper("or", Box::new(helpers::or_helper));
    handlebars.register_helper("yaml-string", Box::new(helpers::yaml_string_helper));
    handlebars.register_helper("url-rm-slash", Box::new(helpers::url_rm_slash_helper));
    handlebars.register_helper("url-add-slash", Box::new(helpers::url_add_slash_helper));
    handlebars.register_helper("url-rm-path", Box::new(helpers::url_rm_path));
    handlebars.register_helper("lowercase", Box::new(helpers::lowercase_string_helper));

    handlebars
}
