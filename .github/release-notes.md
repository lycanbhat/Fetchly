**Fetchly** — a fast, private, open-source API client for macOS, Windows, Linux and the web.

No account. No cloud. Your collections, environments and cookies stay on your machine.

## Download

| Platform | File |
|---|---|
| **macOS** 11+ (Apple Silicon & Intel) | `Fetchly-{{VERSION}}-macos-universal.dmg` |
| **Windows** 10 / 11 (x64) — installer | `Fetchly-{{VERSION}}-windows-x64-setup.exe` |
| **Windows** 10 / 11 (x64) — portable | `Fetchly-{{VERSION}}-windows-x64-portable.zip` |
| **Linux** — Debian / Ubuntu | `fetchly_{{VERSION}}_amd64.deb` |
| **Linux** — other distributions (x86_64) | `Fetchly-{{VERSION}}-linux-x86_64.tar.gz` |

`SHA256SUMS.txt` lists checksums for every file.

## Install

**macOS** — open the `.dmg` and drag **Fetchly** into **Applications**. The app isn't notarized yet, so the first time: right-click Fetchly → **Open** → **Open** (or run `xattr -cr /Applications/Fetchly.app`).

**Windows** — run the installer (no admin rights needed). On the *Windows protected your PC* screen click **More info → Run anyway**. Fetchly uses Microsoft Edge WebView2, which is built into Windows 11 and most Windows 10 PCs; the installer offers the download if it's missing. For the portable zip, keep `WebView2Loader.dll` next to `Fetchly.exe`.

**Linux (Debian / Ubuntu)** — `sudo apt install ./fetchly_{{VERSION}}_amd64.deb`, then launch Fetchly from your app menu.

**Linux (other)** — extract the `.tar.gz` and run `./install.sh` (installs to `~/.local`, no root). Requires the WebKitGTK 4.1 runtime (`webkit2gtk-4.1`), which most desktop distributions include.

## Highlights

- Requests with every common method, params, headers, auth (Basic, Bearer, API key, OAuth 2.0 token) and bodies (JSON, form-data, urlencoded, binary, GraphQL)
- Collections and folders with **drag-and-drop**, environments, globals and `{{variables}}`
- Pre-request and test scripts, and a **collection runner** with live results
- **Cookie manager**, response viewer with search, history, console
- Code generation in 9 languages
- **Import and export Postman** collections (v2.0 / v2.1) and environments
- Light and dark themes

Read the full documentation in the [README](https://github.com/lycanbhat/Fetchly#readme).
