use std::env;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use objc2::{MainThreadMarker, rc::Retained, rc::autoreleasepool};
use objc2_app_kit::{NSAlert, NSApplication, NSRunningApplication};
use objc2_foundation::{NSBundle, NSString, NSURL};
use serde::Deserialize;
use url::Url;

const CONFIG_FILE: &str = ".config/chatgpt_app_launcher.ason";
const DEFAULT_PROXY: &str = "http://localhost:8118";
const APP_NAME: &str = "ChatGPT.app";

#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Config {
    app_path: String,
    proxy: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            app_path: String::new(),
            proxy: DEFAULT_PROXY.to_owned(),
        }
    }
}

fn read_config(path: &Path) -> Result<Config, String> {
    match fs::read_to_string(path) {
        Ok(source) => ason::de_from_str(&source).map_err(|error| {
            format!(
                "Invalid ASON format in configuration file {}.\n{error}",
                path.display()
            )
        }),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(Config::default()),
        Err(error) => Err(format!(
            "Cannot read configuration file {}.\n{error}",
            path.display()
        )),
    }
}

fn parse_proxy(value: &str) -> Result<String, &'static str> {
    const INVALID: &str =
        "proxy must be an HTTP or HTTPS proxy address, such as http://localhost:8118.";
    let value = value.trim();
    let Some((_, authority)) = value.split_once("://") else {
        return Err(INVALID);
    };
    let authority = authority.strip_suffix('/').unwrap_or(authority);
    if authority.is_empty()
        || value.chars().any(|c| c.is_whitespace() || c.is_control())
        || authority.contains(['/', '@', '\\', ';', ',', '=', '?', '#'])
    {
        return Err(INVALID);
    }

    let url = Url::parse(value).map_err(|_| INVALID)?;
    if !matches!(url.scheme(), "http" | "https") || !url.has_host() || url.port() == Some(0) {
        return Err(INVALID);
    }

    // Chromium's proxy flag takes scheme://host[:port], without a URL path.
    Ok(url.origin().ascii_serialization())
}

fn resolve_path(value: &str, home: &Path) -> Result<PathBuf, &'static str> {
    let path = Path::new(value);
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else if let Ok(relative) = path
        .strip_prefix("~")
        .or_else(|_| path.strip_prefix("$HOME"))
    {
        Ok(home.join(relative))
    } else {
        Err("app_path must be an absolute path or begin with ~/ or $HOME/.")
    }
}

struct TargetApp {
    path: PathBuf,
    identifier: Retained<NSString>,
}

impl TargetApp {
    fn at(path: &Path) -> Option<Self> {
        if !path.extension()?.eq_ignore_ascii_case("app") || !path.is_dir() {
            return None;
        }
        let url = NSURL::from_file_path(path)?;
        let bundle = NSBundle::bundleWithURL(&url)?;
        let executable = bundle.executableURL()?.to_file_path()?;
        if !executable.is_file() {
            return None;
        }
        Some(Self {
            path: path.to_path_buf(),
            identifier: bundle.bundleIdentifier()?,
        })
    }
}

fn find_app(home: &Path, applications: &Path, configured: &str) -> Result<TargetApp, String> {
    if !configured.is_empty() {
        let path = resolve_path(configured, home)?;
        return TargetApp::at(&path).ok_or_else(|| {
            format!(
                "The configured app_path is not a launchable macOS application.\n{}",
                path.display()
            )
        });
    }

    let candidates = [
        applications.join(APP_NAME),
        home.join("Applications").join(APP_NAME),
    ];
    candidates.iter().find_map(|path| TargetApp::at(path)).ok_or_else(|| {
        format!(
            "Cannot find ChatGPT.app. Checked the following paths:\n{}\n{}\n\nYou can specify app_path in ~/{CONFIG_FILE}.",
            candidates[0].display(),
            candidates[1].display(),
        )
    })
}

fn launch_command(app_path: &Path, proxy: &str) -> Command {
    let mut command = Command::new("/usr/bin/open");
    command.arg("-a").arg(app_path);
    for name in ["http_proxy", "https_proxy", "HTTP_PROXY", "HTTPS_PROXY"] {
        command.arg("--env").arg(format!("{name}={proxy}"));
    }
    command.arg("--args").arg(format!("--proxy-server={proxy}"));
    command
}

fn launch() -> Result<(), String> {
    let home = env::home_dir().ok_or("Cannot find the user's home directory.")?;
    let config_path = home.join(CONFIG_FILE);
    let config = read_config(&config_path)?;
    let proxy = parse_proxy(&config.proxy)
        .map_err(|error| format!("{}\n{error}", config_path.display()))?;
    let app = find_app(&home, Path::new("/Applications"), &config.app_path)?;

    if !NSRunningApplication::runningApplicationsWithBundleIdentifier(&app.identifier).is_empty() {
        return Err(
            "ChatGPT is already running. Press ⌘Q in ChatGPT to quit it completely, then reopen the launcher to apply the proxy."
                .to_owned(),
        );
    }

    let output = launch_command(&app.path, &proxy)
        .output()
        .map_err(|error| format!("Cannot execute the macOS open command.\n{error}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "Cannot launch {}.\n{}\n{}",
            app.path.display(),
            output.status,
            stderr.trim()
        ));
    }
    Ok(())
}

fn main() -> ExitCode {
    autoreleasepool(|_| match launch() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            if NSBundle::mainBundle().bundleIdentifier().is_some() {
                let mtm = MainThreadMarker::new().expect("the launcher runs on the main thread");
                let app = NSApplication::sharedApplication(mtm);
                app.activate();
                let alert = NSAlert::new(mtm);
                alert.setMessageText(&NSString::from_str("ChatGPT Proxy Launcher"));
                alert.setInformativeText(&NSString::from_str(&message));
                alert.runModal();
            }
            ExitCode::FAILURE
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;

    #[test]
    fn missing_config_uses_defaults_and_partial_ason_overrides_them() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("launcher.ason");

        let defaults = read_config(&file).unwrap();
        assert!(defaults.app_path.is_empty());
        assert_eq!(defaults.proxy, DEFAULT_PROXY);

        fs::write(
            &file,
            "{\n  // Alternate proxy\n  proxy: \"http://127.0.0.1:9000\"\n}\n",
        )
        .unwrap();
        let config = read_config(&file).unwrap();
        assert!(config.app_path.is_empty());
        assert_eq!(config.proxy, "http://127.0.0.1:9000");

        fs::write(&file, "{ app_path: \"~/My Apps/ChatGPT.app\" }").unwrap();
        let config = read_config(&file).unwrap();
        assert_eq!(config.app_path, "~/My Apps/ChatGPT.app");
        assert_eq!(config.proxy, DEFAULT_PROXY);

        fs::write(&file, "{ proxxy: \"http://localhost:8118\" }").unwrap();
        assert!(read_config(&file).is_err());
    }

    #[test]
    fn configured_path_overrides_system_then_home_search() {
        let dir = tempfile::tempdir().unwrap();
        let system = dir.path().join("Applications");
        let home = dir.path().join("home");
        let user = home.join("Applications").join(APP_NAME);
        write_test_app(&user, "org.example.user-chatgpt");

        assert_eq!(find_app(&home, &system, "").unwrap().path, user);

        let global = system.join(APP_NAME);
        write_test_app(&global, "org.example.system-chatgpt");
        assert_eq!(find_app(&home, &system, "").unwrap().path, global);
        assert_eq!(
            find_app(&home, &system, "$HOME/Applications/ChatGPT.app")
                .unwrap()
                .path,
            user
        );
        assert!(find_app(&home, &system, "~/missing.app").is_err());
    }

    fn write_test_app(path: &Path, identifier: &str) {
        let contents = path.join("Contents");
        fs::create_dir_all(contents.join("MacOS")).unwrap();
        fs::write(contents.join("MacOS/ChatGPT"), b"#!/bin/sh\n").unwrap();
        fs::write(
            contents.join("Info.plist"),
            format!(
                r#"<?xml version="1.0" encoding="UTF-8"?>
            <plist version="1.0"><dict>
                <key>CFBundleExecutable</key><string>ChatGPT</string>
                <key>CFBundleIdentifier</key><string>{identifier}</string>
                <key>CFBundlePackageType</key><string>APPL</string>
            </dict></plist>"#,
            ),
        )
        .unwrap();
    }

    #[test]
    fn proxy_is_passed_to_both_electron_and_the_launched_environment() {
        let proxy = parse_proxy("http://[::1]:8118/").unwrap();
        let app = Path::new("/Applications/Chat GPT.app");
        let command = launch_command(app, &proxy);
        assert_eq!(command.get_program(), "/usr/bin/open");
        assert!(
            command.get_args().eq([
                "-a",
                "/Applications/Chat GPT.app",
                "--env",
                "http_proxy=http://[::1]:8118",
                "--env",
                "https_proxy=http://[::1]:8118",
                "--env",
                "HTTP_PROXY=http://[::1]:8118",
                "--env",
                "HTTPS_PROXY=http://[::1]:8118",
                "--args",
                "--proxy-server=http://[::1]:8118",
            ]
            .into_iter()
            .map(OsStr::new))
        );

        for invalid in [
            "localhost:8118",
            "http:///localhost:8118",
            "http://localhost:0",
            "http://localhost:8118/x",
            "http://localhost:8118/x/..",
            "http://user:secret@localhost:8118",
            "http://localhost:8118?x=y",
        ] {
            assert!(parse_proxy(invalid).is_err(), "accepted {invalid}");
        }
    }
}
