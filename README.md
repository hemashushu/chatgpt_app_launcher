# ChatGPT Proxy Launcher

This is a macOS launcher written in Rust. Double-click it to open ChatGPT using an existing proxy. The proxy address is passed to the launched ChatGPT application and its Chromium network stack; no changes to macOS system proxy settings are needed.

## Temporary solution

This launcher is a temporary workaround. Once the ChatGPT app supports configuring a proxy directly, this launcher will no longer be needed.

## How it works

The launcher reads its optional configuration, validates the HTTP or HTTPS proxy address, and locates the ChatGPT application. It checks the application's bundle identifier for an existing running instance and stops if one is found, because an existing process cannot receive the new launch settings.

The launcher then uses macOS's `/usr/bin/open` command to start the application. It passes the proxy address through the `http_proxy`, `https_proxy`, `HTTP_PROXY`, and `HTTPS_PROXY` environment variables and the `--proxy-server=<proxy>` argument for Chromium's network stack. These settings apply to the launched application and components that honor them; the launcher does not run a proxy server or change macOS system proxy settings.

For example, when configured to use `~/Applications/ChatGPT.app` and `http://127.0.0.1:8118`, the launch is roughly equivalent to:

```sh
open -a "$HOME/Applications/ChatGPT.app" \
  --env http_proxy=http://127.0.0.1:8118 \
  --env https_proxy=http://127.0.0.1:8118 \
  --args --proxy-server=http://127.0.0.1:8118
```

## Build and usage

Install Rust and Xcode Command Line Tools on macOS, then run the following command in the project directory.

```sh
./scripts/package-macos.sh
```

The resulting application is located at `target/macos/ChatGPT Proxy Launcher.app`. Double-click it in Finder or copy it to your Applications directory. The packaging script builds for the native architecture and applies an ad hoc signature.

Keep the proxy service running. If ChatGPT is already open, press **⌘Q** in ChatGPT to quit it completely before opening the launcher. A running process cannot receive new proxy arguments, so the launcher will prompt you to quit it.

## Configuration

The launcher reads `~/.config/chatgpt_app_launcher.ason` each time it starts. The file is optional, and both fields may be omitted.

```ason
{
    app_path: "~/Applications/ChatGPT.app"
    proxy: "http://localhost:8118"
}
```

- `app_path` supports absolute paths and paths beginning with `~/` or `$HOME/`. If omitted or set to an empty string, the launcher checks `/Applications/ChatGPT.app` and then `$HOME/Applications/ChatGPT.app`.
- `proxy` accepts addresses in the form `http://host:port` or `https://host:port`. The default is `http://localhost:8118`. Use `https://` to connect to the proxy server itself over TLS. Include only the host and port; do not include a username, password, or URL path.

The launcher displays a specific error if the configuration file cannot be read, its format is invalid, or the explicitly specified application is unavailable. See the [ASON format documentation](https://github.com/hemashushu/ason).

## Testing

```sh
cargo test --locked
```

## License

This project is licensed under the [MIT License](LICENSE). The packaged application also includes the license in `Contents/Resources/LICENSE`.
