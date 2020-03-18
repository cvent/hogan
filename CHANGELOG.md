## v0.7.4
[Full Changelog](https://github.com/cvent/hogan/compare/v0.7.3...v0.7.4)

* 5fd4fd0 Merge pull request #42 from jjcomer/dep-upgrade
* b0ca289 ci: only perform release builds and tests
* 1d88298 fix: clean mac env on travis
* 16e6b08 fix: pin cross version
* d85a34d ci: version bump
* 2afcc35 chore: upgrade deps

## v0.7.3
[Full Changelog](https://github.com/cvent/hogan/compare/v0.7.2...v0.7.3)

* 5dd85fa Merge pull request #41 from jjcomer/branch-transform
* b7ecb0b feat: add branch based template
* 0ae6bc4 chore: update deps

## v0.7.2
[Full Changelog](https://github.com/cvent/hogan/compare/v0.7.1...v0.7.2)

* c834f2f Merge pull request #40 from jjcomer/refactor
* 2e7296e Merge pull request #23 from dinkarthakur/documentation-update
* ad73498 chore: update version
* 132f29f refactor: divided cli from server
* 4f65f88 chore: update deps
* 64e0db8 Change to create function instead of alias to have parameterized command
* 3b28f63 Added documentation to add alias for the hogan command to that we need not to type long command

## v0.7.1
[Full Changelog](https://github.com/cvent/hogan/compare/v0.7.0...v0.7.1)

* af96a49 Merge pull request #39 from jjcomer/actix2
* e89ffee Inc version
* beb8383 Upgrade to actix-web 2.0.0

## v0.7.0
[Full Changelog](https://github.com/cvent/hogan/compare/v0.6.0...v0.7.0)

* cb6793a Merge pull request #38 from jjcomer/branch-head
* fb1422d Reduce params as per clippy
* 68385d1 Add initial timing metrics
* cc953d0 Re-enable travis caching
* bcc955f :construction_worker: build on stable rust
* 3ceea3c :building_construction: Convert to actix-web
* 89f0170 Update deps
* f2c3ee5 Improve branch parsing
* 0523a8f Add branch-type query and remove lambda support
* 777eb11 Merge pull request #37 from jjcomer/master
* 2f384ea Change gauge metric to a timer
* 71d9d97 Allow nightly binary artifacts

## v0.6.0
[Full Changelog](https://github.com/cvent/hogan/compare/v0.5.0...v0.6.0)

* 3fa98e8 Merge pull request #36 from jjcomer/memory-try-3
* 68c1a12 Change caching strategy and remove template all
* f8a00db Improve memory consumption
* db10e25 Update structopt

## v0.5.0
[Full Changelog](https://github.com/cvent/hogan/compare/v0.4.3...v0.5.0)

* 901397f chore: version bump
* bff4cb1 Merge pull request #34 from ischell/make-or-helper-multi
* 0b95364 Merge branch 'master' into make-or-helper-multi
* ebba233 Merge pull request #1 from cvent/master
* 5266c4e Dd monitor custom metrics (#31)
* 57e0bf9 Added test for error condition on or helper
* 256aed5 Switch the or helper to take in multiple values

## v0.4.3
[Full Changelog](https://github.com/cvent/hogan/compare/v0.4.2...v0.4.3)

* e568dcc Merge pull request #32 from jjcomer/master
* eeaf972 Fix refreshing the git repo

## v0.4.2
[Full Changelog](https://github.com/cvent/hogan/compare/v0.4.1...v0.4.2)

* 76cec1d Merge pull request #29 from jjcomer/master
* 7a69c6b Add ability to specify regex for configs on the server

## v0.4.1
[Full Changelog](https://github.com/cvent/hogan/compare/v0.4.0...v0.4.1)

* a8e6f0c Version bump
* 1832600 Merge pull request #28 from jjcomer/master
* 8c53e2d Add ability to bind server address
* 5ef7006 Use discover instead of open to find git repo

## v0.4.0
[Full Changelog](https://github.com/cvent/hogan/compare/v0.3.0...v0.4.0)

* 067cdb1 Merge pull request #27 from jjcomer/lambda
* 11b2620 Update travis ci distro
* 5021575 Missed moving travis to nightly
* e0c9661 Changed web server to rocket

## v0.3.0
[Full Changelog](https://github.com/cvent/hogan/compare/v0.2.10...v0.3.0)

* 4bed3ed Merge pull request #25 from jjcomer/master
* 1024f76 Fix divide by zero error
* 5a115ee Add property for env regex to server config
* ccec8a5 Add cache updating to getEnvs
* e428e2f Remove hard coded origin/master
* 924f31d Clippy fixup
* 06621c9 Add health check route
* b1f9fbe BREAKING -- Support multiple git SHA targets
* 9ab7340 Merge pull request #24 from jjcomer/upgrade-edition
* a964911 Fix clippy warnings
* 37714be Fix formatting
* 1669ac8 Upgrade to 2018
* 003a505 Update deps

## v0.2.10
[Full Changelog](https://github.com/cvent/hogan/compare/v0.2.9...v0.2.10)

* d74bb88 version bump
* e581bd9 Merge pull request #22 from rlopezpe/fix-template-file-parsing-regex
* 450ce29 add test fixtures to test template files with a names in the format of 'template.*'
* 6382f8f fixes #21 - update unit tests to check for templates with name starting with 'template'
* fe3326b escape backslash in template regex
* 98c3f88 fixes #21 template parsing regex to match template files with a name starting with 'template'
* f8580e5 Replace MacOS instructions with brew instructions

## v0.2.9
[Full Changelog](https://github.com/cvent/hogan/compare/v0.2.8...v0.2.9)

* 5b70827 Version bump
* 334bf07 Merge pull request #18 from mpdatx/master
* 2639133 Change the template regex so it ignores files that start with a dot Added a testcase for this behavior in project-3.

## v0.2.8
[Full Changelog](https://github.com/cvent/hogan/compare/v0.2.7...v0.2.8)

* 3388e65 Version bump
* 62aa702 Merge pull request #16 from adamjones83/ignore_existing2
* 988658a add an ignore-existing flag to skip rather than overwrite existing configs
* b65f152 Merge pull request #14 from ed-norris-cvent/patch-1
* 0efd90b Fixed example in README

## v0.2.7
[Full Changelog](https://github.com/cvent/hogan/compare/v0.2.6...v0.2.7)

* 55d6190 Version bump
* 52e9e22 Merge pull request #12 from cvent/password-auth
* eedc547 Support password auth
* 2daba41 Move to alpine to support being used in a multi-stage Dockerfile
* 3301049 Merge pull request #11 from cvent/linux-install
* 9d3aef1 Add linux install instructions

## v0.2.6
[Full Changelog](https://github.com/cvent/hogan/compare/v0.2.5...v0.2.6)

* a48c909 Merge pull request #10 from cvent/git-file-paths
* db67f9b Support git file paths, also bump crate versions
* b20dc74 Hogan is not XML specific
* fbd84e7 Add openssl instructions to MacOS install

## v0.2.5
[Full Changelog](https://github.com/cvent/hogan/compare/v0.2.4...v0.2.5)

* cba891b Version bump
* b6c83fd Merge pull request #9 from cvent/windows-gnu
* da71da2 Disable test for windows x64
* bf07e71 Remove the unstable flag from cargo configuration
* da0fc1b Add cargo config for statically linking msvc
* 90a2944 log mingw
* 80cfad9 log msys64
* 05a16d8 remove other targets, add logging
* 5079db6 Tweak the build matrix. Disable nightlies, and re-enable windows GNU

## v0.2.4
[Full Changelog](https://github.com/cvent/hogan/compare/v0.2.3...v0.2.4)

* d3a88b1 Version bump
* 45a86d8 Merge pull request #8 from cvent/cli-tests
* 3d1057e This is not running in bash
* 79c971c Use custom image
* 5d1b031 Move zlib install to install.sh
* 4b9cdc1 Try explicitly installing zlib
* 6bd5286 Remove unused uses
* 67321b5 Copy directory to tmp to avoid read only FS
* 809912b Remove unused uses
* 8c6f2e4 Fix tests
* cde0780 Add CLI tests

## v0.2.3
[Full Changelog](https://github.com/cvent/hogan/compare/v0.2.2...v0.2.3)

* 75c9f26 Version bump
* 8c92ba2 Merge pull request #5 from cvent/add-docker
* 94f009c Add docker support

## v0.2.2
[Full Changelog](https://github.com/cvent/hogan/compare/v0.2.1...v0.2.2)

* 7e511ab Version bump
* a51d2ff Add docs for the `or` helper
* 8e5666b Merge pull request #4 from cvent/or-helper
* f48f265 Add or helper

## v0.2.1
[Full Changelog](https://github.com/cvent/hogan/compare/v0.2.0...v0.2.1)

* ca7c90a Version bump
* babec6f Merge pull request #3 from cvent/accept-non-urls
* 18919ab Allow non-URLs to pass through url_rm_path

## v0.2.0
[Full Changelog](https://github.com/cvent/hogan/compare/v0.1.0...v0.2.0)

* 5298e56 Version bump
* 1e2435d Add short commands
* 94dab66 Update README.md
* 865d9b2 Add more short options
* 06da7db Fix usage in README
* a6f5a38 Merge pull request #1 from cvent/server
* e880c3f Don't build windows mingw
* b95fb21 Fix directory detection
* 6b69188 Remove unnecessary match
* cc517d3 Improve git repo handling
* 9eb3447 Add server

## v0.1.0
[Full Changelog](https://github.com/cvent/hogan/compare/060a7d9e459818ec39a72abec409a6800cf53bf5...v0.1.0)

* 513367f Reluctanlty disable skeptic, it is failing to find a crate on travis/appveyor
* a1fd678 Amend gitignore to commit lockfile and keep test templates
* 892952e Initial commit

