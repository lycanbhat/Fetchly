# ⚡ Fetchly

A fast, open-source API client written in Rust with [Dioxus](https://dioxuslabs.com). One codebase runs as a native desktop app on **Windows, macOS and Linux**, and in the **browser** via WebAssembly.

Developed by [Akshay Bhat](https://akshaybhat.online).

## Features

| Area | What you get |
|---|---|
| **Requests** | GET, POST, PUT, PATCH, DELETE, HEAD, OPTIONS · multiple tabs · URL ⇄ query-param table sync · paste a cURL command into the URL bar to import it |
| **Body** | none · raw (JSON / Text / XML / HTML / JavaScript, with JSON validation and *Beautify*) · form-data (text and file fields) · x-www-form-urlencoded · binary file · GraphQL (query + variables) |
| **Auth** | Inherit from parent · No Auth · Basic · Bearer · API Key (header or query) · OAuth 2.0 access token. Set on a collection or folder and inherited by its requests |
| **Variables** | `{{var}}` everywhere, highlighted green (defined) or red (missing), with a warning for undefined variables in the URL · precedence: local → environment → collection → global · dynamic variables like `{{$guid}}`, `{{$timestamp}}`, `{{$isoTimestamp}}`, `{{$randomInt}}`, `{{$randomEmail}}`… |
| **Environments** | Create, duplicate, switch, import, export · globals |
| **Collections** | Nested folders · **drag-and-drop to reorder or move** requests, folders and collections (including across collections) · rename, duplicate, delete from a menu or right-click · collection & folder-level auth, scripts, variables, descriptions |
| **Cookies** | A shared cookie jar used by every request · **cookie manager**: browse by domain, add, edit (value, path, expiry, Secure, HttpOnly), delete, clear · cookies persist across restarts |
| **Scripts & tests** | Pre-request and test scripts at collection, folder and request level, with a `pm` API (`pm.test`, `pm.expect(...)`, `pm.environment.set`, `pm.response.json()`, `pm.request.headers.upsert`, `pm.execution.setNextRequest`…) and built-in snippets |
| **Response** | Status, time, size · Pretty (JSON/XML/HTML) and Raw views with syntax highlighting · find in body with highlighted matches · headers · cookies · test results · the exact request sent · copy or save to file |
| **Collection Runner** | Run a collection or folder with iterations, delay, stop-on-failure and `setNextRequest` flow control · live pass/fail report · export results as JSON |
| **Code generation** | cURL, Python requests, JavaScript fetch, Node axios, Rust reqwest, Go net/http, raw HTTP, HTTPie, PowerShell |
| **Import / export** | Postman Collection v2.0/v2.1 and Postman environment JSON (both directions) · cURL · full workspace backup |
| **Other** | Bulk-edit for key/value tables · history grouped by day · console with request/response details and script logs · SSL verification, redirect and timeout settings per request · light/dark/system theme · interface scale · resizable panes · autosave |

### UI stack

- **[Dioxus 0.7](https://dioxuslabs.com)** for the UI (a webview on desktop, WebAssembly in the browser).
- **[Dioxus components](https://github.com/DioxusLabs/components)** (dialogs, dropdown menus, switches…) built on `dioxus-primitives`, vendored in `crates/app/src/components/` (MIT / Apache-2.0).
- **[Lucide](https://lucide.dev)** icons via `lucide-dioxus`.
- **[Geist](https://vercel.com/font)** and Geist Mono fonts (SIL Open Font License), embedded in the binary.

All CSS and fonts are compiled into the executable, so the desktop app is a single self-contained file.

## Project layout

```
crates/
  core/    fetchly-core: models, variables, HTTP, cookies, scripting, cURL, codegen, Postman import/export
           (platform-independent, compiles to native and wasm32)
  app/     fetchly: the Dioxus UI (desktop + web)
    src/components/   vendored Dioxus components
    src/ui/           screens: sidebar, editor, response, dialogs, chrome
    styles/           component theme + app stylesheet
  server/  fetchly-server: serves the web build and provides a CORS-free request relay
```

## Run on desktop

Requires [Rust](https://rustup.rs) (stable).

```sh
cargo run --release            # launches the desktop app
```

**Linux** needs the WebKitGTK stack:
`sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libxdo-dev libayatana-appindicator3-dev librsvg2-dev`

**Windows** uses the Microsoft Edge WebView2 runtime, which is built into Windows 11 and present on most Windows 10 PCs.

## Build installers to share

| Platform | Command | Output (in `target/`) |
|---|---|---|
| macOS (Apple Silicon + Intel) | `./scripts/bundle-macos.sh universal` | `macos/Fetchly-<version>.dmg` |
| Windows x64 (cross-built from macOS) | `./scripts/build-windows.sh` | `windows/Fetchly-Setup-<version>.exe`, `windows/Fetchly-<version>-portable.zip` |

The Windows build needs a one-time setup on macOS: `brew install mingw-w64 makensis`. The installer installs per user (no admin rights), adds Start Menu and Desktop shortcuts and an uninstaller, and offers the WebView2 download if a PC doesn't have it. Neither build is code-signed, so macOS Gatekeeper and Windows SmartScreen will warn on first launch.

## Run on the web

```sh
rustup target add wasm32-unknown-unknown
rustup component add llvm-tools
cargo install dioxus-cli@0.7.10 --locked

dx build --platform web --release -p fetchly           # run from the repo root
cargo run --release -p fetchly-server -- --dist target/dx/fetchly/release/web/public
# → http://127.0.0.1:8080
```

For live development, use `dx serve --platform web -p fetchly`. In the browser, set the proxy endpoint under **Settings → Network** to a running `fetchly-server`.

Browsers block most cross-origin API calls (CORS) and forbid some headers. So the web app sends requests through `fetchly-server`'s `POST /api/proxy` by default, which behaves like the desktop app (including following redirects and keeping cookies). If you host the web build on a static host without the server, turn the proxy off in **Settings → Network** and only call APIs that allow CORS.

`fetchly-server` options: `--host` (default `127.0.0.1`), `--port` (default `8080`, or `$PORT`), `--dist` (default `./dist`).

> **Security:** the proxy makes HTTP requests on behalf of whoever can reach it. If you bind it to a public interface, set `FETCHLY_PROXY_TOKEN=<secret>` and enter the same token under **Settings → Network → Proxy token**.

### Docker

```sh
docker build -t fetchly .
docker run -p 8080:8080 -e FETCHLY_PROXY_TOKEN=change-me fetchly
```

## Scripting

Scripts are written in [Rhai](https://rhai.rs), a small, sandboxed, JavaScript-like language that runs identically on desktop and web. Closures use `|| expr` instead of `function () {}`:

```js
// Tests tab
pm.test("Status is 200", || pm.response.to_have_status(200));
pm.test("Returns items", || {
    let data = pm.response.json();
    pm.expect(data.items).to_have_length(3);
    pm.expect(data.total).to_be_above(0);
});
pm.environment.set("token", pm.response.json().access_token);

// Pre-request tab
pm.request.headers.upsert("X-Request-Id", uuid());
```

Click **Reference** above any script editor for the full API. Scripts imported from Postman collections are JavaScript. They're kept as comments so you can port them, and nothing breaks.

Infinite loops are cut off automatically (operation limit), so a bad script can't freeze the app.

## Keyboard shortcuts

| Shortcut | Action |
|---|---|
| Ctrl/⌘ + Enter | Send |
| Ctrl/⌘ + S | Save |
| Ctrl/⌘ + T | New tab |
| Ctrl/⌘ + O | Import |
| Middle-click a tab | Close it |
| Double-click a sidebar item | Rename |
| Right-click a sidebar item | Actions menu |

## Data storage

Everything (collections, environments, cookies, history, open tabs, settings) autosaves locally: on desktop in the OS's standard app-data folder (e.g. `~/Library/Application Support/Fetchly/state.json` on macOS), on the web in the browser's local storage. Data from the earlier egui build of Fetchly is migrated automatically on first launch.

Use **Settings → Export backup** to move data between machines or between desktop and web.

## Development

```sh
cargo test --workspace                                                     # engine + UI helper tests
cargo check -p fetchly --target wasm32-unknown-unknown --no-default-features --features web
```

CI (`.github/workflows/build.yml`) builds release binaries for Linux x64, macOS (Apple Silicon and Intel), Windows x64, and the web bundle.
