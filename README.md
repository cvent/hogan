[![Build Status](https://travis-ci.org/cvent/hogan.svg?branch=master)](https://travis-ci.org/cvent/hogan)
[![Build status](https://ci.appveyor.com/api/projects/status/xtdavsrk8fs27uox/branch/master?svg=true)](https://ci.appveyor.com/project/jonathanmorley/hogan/branch/master)

# hogan

## Purpose

The purpose of this project is to generate config overrides so that we can keep a template up to date, and populate values on the fly with ease at build time.

## Installation

Grab a binary for your OS from the [latest release](https://github.com/cvent/hogan/releases/latest), and put it somewhere in your PATH.

### MacOS

```sh
brew tap cvent/tap
brew install hogan
```

### Linux

```sh
curl -LSfs https://japaric.github.io/trust/install.sh | sh -s -- --git cvent/hogan --target x86_64-unknown-linux-gnu --to /usr/local/bin
```

## Tests

You can run the tests via `cargo test`. The tests should always pass and all new behavior should be tested.

## Usage

Once you have installed hogan, you can execute it as `hogan`.
Some of the arguments are described below:

 * `environments-filter`: Regex specifying which environment(s) to update.
 * `templates`: The directory to use for searching for template files (recursively).
 * `configs`: The directory where hogan-formatted config files can be found (These are config.ENVIRONMENT.json files)

## Example

```
    hogan transform --environments-filter ENVIRONMENT --templates . --configs ./Configs
```

## Custom handlers in config files

The following custom handlers exist

### `comma-list`
Allows an array of objects to be turned into a comma separated list by passing in an array:

```rust,skt-helpers
// Given a config of:
let config = json!({
  "Memcache": {
    "Servers": [
      {
        "Endpoint": "192.168.1.100",
        "Port": "1122"
      },
      {
        "Endpoint": "192.168.1.101",
        "Port": "1122"
      },
      {
        "Endpoint": "192.168.1.102",
        "Port": "1122"
      }
    ]
  }
});

// and a template of:
let template = "{{#comma-list Memcache.Servers}}{{Endpoint}}:{{Port}}{{/comma-list}}";

// The helper will transform it into:
let transformed = "192.168.1.100:1122,192.168.1.101:1122,192.168.1.102:1122";
```

### `equal`, `eq`
Like `if`, but compares the two arguments provided for equality:

```rust,skt-helpers
// Given a config of:
let config = json!({
  "Region": {
    "Key": "TEST"
  }
});

// and a template of:
let template = r#"{{#equal Region.Key "TEST"}}True{{else}}False{{/equal}}"#;

// The helper will transform it into:
let transformed = "True";
```

### `or`
Logical OR two parameters:

```rust,skt-helpers
// Given a config of:
let config = json!({
  "Region": {
    "Key": "TEST"
  }
});

// and a template of:
let template = r#"{{#or (eq Region.Key "TEST") (eq Region.Key "TEST2")}}True{{else}}False{{/or}}"#;

// The helper will transform it into:
let transformed = "True";
```

### `yaml-string`
Escapes a string for valid injection into a Yaml file:

```rust,skt-helpers
// Given a config of:
let config = json!({
  "app": {
    "path": "C:\\Program Files\\My App"
  }
});

// and a template of:
let template = r#"windows:
  path: "{{yaml-string app.path}}""#;

// The helper will transform it into:
let transformed = r#"windows:
  path: "C:\\Program Files\\My App""#;
```

### `url-rm-slash`
Removes the trailing slash on an endpoint:

```rust,skt-helpers
// Given a config of:
let config = json!({
  "SlashService": {
    "endpoint": "https://slash.com/"
  }
});

// and a template of:
let template = "{{url-rm-slash SlashService.endpoint}}";

// The helper will transform it into:
let transformed = "https://slash.com";
```

### `url-add-slash`
Adds the trailing slashes on an endpoint:

```rust,skt-helpers
// Given a config of:
let config = json!({
  "NonSlashService": {
    "endpoint": "https://nonslash.com"
  }
});

// and a template of:
let template = "{{url-add-slash NonSlashService.endpoint}}";

// The helper will transform it into:
let transformed = "https://nonslash.com/";
```

### `url-rm-path`
Removes the last slash plus content to the end of the string:

```rust,skt-helpers
// Given a config of:
let config = json!({
  "PathService": {
    "endpoint": "https://path.com/path/remove-this"
  }
});

// and a template of:
let template = "{{url-rm-path PathService.endpoint}}";

// The helper will transform it into:
let transformed = "https://path.com/path";
```

## Helpful Information

 - [Handlebars](http://handlebarsjs.com/)
