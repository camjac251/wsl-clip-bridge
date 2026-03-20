# Changelog

## [1.0.1](https://github.com/camjac251/wsl-clip-bridge/compare/v1.0.0...v1.0.1) - 2026-03-20

### Fixed

- *(release)* add git_release_enable and git_tag_enable to release-plz

### Other

- release v1.0.0

## [1.0.0](https://github.com/camjac251/wsl-clip-bridge/releases/tag/v1.0.0) - 2026-03-20

### Added

- add --help/--version flags and dynamic version embedding
- add wl-clipboard integration for direct Windows clipboard support
- enhance setup script with smart update detection and config preservation
- add MIME type detection to ShareX integration
- add changelog generation and cleaner version numbering
- initial implementation

### Fixed

- *(release)* add explicit publish=false to release-plz config
- *(release)* use publish_no_verify instead of invalid cargo_package field
- *(release)* disable cargo package verify in release-plz
- *(deps)* update rust crate toml to v1
- *(wl-clipboard)* resolve pipe buffer deadlock causing Weston crashes
- add 2-second timeout to wl-paste commands
- *(setup)* allow dots in WSL distribution names
- *(deps)* update rust crate serde to v1.0.226
- *(deps)* update rust crate toml to v0.9.7
- default to yes for ShareX close/reopen prompts
- separate TARGETS handling from default output mode
- *(deps)* update rust crate serde to v1.0.223
- improve commit message handling in release workflow
- PowerShell setup script path handling and batch file generation
- allow empty WSLDistribution parameter for iex usage
- PowerShell setup script improvements
- add checkout step to release job for commit info
- add explicit permissions to workflows
- release workflow artifact handling

### Other

- *(release)* switch to release-plz and standardize CI
- *(deps)* update rust crate toml to v1.0.6
- *(deps)* update rust crate toml to v0.9.11
- add explicit config setup instructions
- add mise and user-local installation options
- streamline README and standardize MIT license
- exclude shell scripts from language stats
- clean up gitattributes for project structure
- add lefthook pre-commit hooks and fix formatting
- *(deps)* update rust crate toml to v0.9.10
- *(deps)* update rust crate toml to v0.9.9
- switch to musl for static linking
- Merge pull request #10 from camjac251/renovate/serde-monorepo
- *(deps)* update github artifact actions
- format workflow files with prettier for better readability
- add validation step before release builds
- exclude scripts and docs from triggering CI workflow
- format README with prettier
- format README description on separate line
- trigger release build on Cargo.lock changes
- format GitHub workflow YAML files
- replace restrict_to_home with allowed_directories security model
- add terminal Ctrl+V forwarding configuration instructions
- add manual ShareX setup with batch file template
- add Rust dependency caching to speed up CI builds
- update alternative solutions description to be more accurate
- improve PowerShell setup script UX and fix clipboard test
- add concurrency controls to prevent duplicate runs
- *(deps)* update actions/download-artifact action to v5
- use actions-rust-cross for simplified cross-compilation
- simplify release notes to focus on changes
- Merge pull request #2 from camjac251/renovate/serde-monorepo
- add workflow timeouts and PR event types
- Add renovate.json

## [2025.12.29.33] - 2025-12-29

### Bug Fixes

- Resolve pipe buffer deadlock causing Weston crashes
## [2025.12.25.32] - 2025-12-25

### Features

- Add --help/--version flags and dynamic version embedding
## [2025.12.25.31] - 2025-12-25

### Bug Fixes

- Add 2-second timeout to wl-paste commands
## [2025.11.21.26] - 2025-11-21

### Bug Fixes

- Allow dots in WSL distribution names
## [2025.10.05.19] - 2025-10-05

### Bug Fixes

- Update rust crate serde to v1.0.226
## [2025.09.19.18] - 2025-09-19

### Bug Fixes

- Update rust crate toml to v0.9.7
- Update rust crate serde to v1.0.225
## [2025.09.15.17] - 2025-09-15

### Features

- Add wl-clipboard integration for direct Windows clipboard support
## [2025.09.14.16] - 2025-09-14

### Bug Fixes

- Update rust crate toml to 0.9
- Update rust crate serde to v1.0.221
- Release workflow artifact handling
- Add explicit permissions to workflows
- Add checkout step to release job for commit info
- PowerShell setup script improvements
- Allow empty WSLDistribution parameter for iex usage
- PowerShell setup script path handling and batch file generation
- Improve commit message handling in release workflow
- Update rust crate serde to v1.0.223
- Separate TARGETS handling from default output mode
- Default to yes for ShareX close/reopen prompts

### Features

- Initial implementation
- Add changelog generation and cleaner version numbering
- Add MIME type detection to ShareX integration
- Enhance setup script with smart update detection and config preservation

### Refactoring

- Simplify release notes to focus on changes
- Use actions-rust-cross for simplified cross-compilation
- Improve PowerShell setup script UX and fix clipboard test
- Replace restrict_to_home with allowed_directories security model
