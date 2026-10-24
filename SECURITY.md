# Security Policy

## About glogger

glogger is a desktop companion app for Project: Gorgon. It reads local game log files and downloads public game data from the Project: Gorgon CDN. It does not handle authentication credentials, payment information, or other sensitive user data beyond local game logs.

## Reporting a Vulnerability

If you discover a security issue, please report it responsibly:

1. **Do not** open a public GitHub issue for security vulnerabilities
2. Email **<danielout@gmail.com>** with a description of the issue
3. Include steps to reproduce if possible
4. Allow reasonable time for a fix before public disclosure

## Scope

Security concerns relevant to glogger include:

- **Local file access**: The app reads Player.log and writes to its own SQLite database in the app data directory. It should not access files outside its expected scope.
- **Network requests**: The app fetches data from `cdn.projectgorgon.com` and `api.github.com` (for update checks). It should not make requests to unexpected endpoints.
- **Tauri security**: CSP configuration, asset protocol scope, and IPC boundaries between the Rust backend and webview frontend.
- **Dependency vulnerabilities**: Issues in third-party Rust crates or npm packages.

## Supported Versions

Only the latest release is supported with security fixes.
