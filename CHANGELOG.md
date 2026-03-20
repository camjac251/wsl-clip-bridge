# Changelog

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
