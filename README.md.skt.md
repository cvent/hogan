```rust,skt-helpers
extern crate hogan;
#[macro_use]
extern crate serde_json;

fn main() {{
  let handlebars = hogan::transform::handlebars();

  {}

  let rendered = handlebars.render_template(template, &config);
  assert!(rendered.is_ok());
  assert_eq!(&rendered.unwrap(), transformed);
}}
```