# Sideglance

Your Claude and Codex usage, Claude Code notifications, the weather and a Pomodoro timer, on a Divoom Ditoo Pro. A desktop app for Windows and macOS that sits in the tray or menu bar.

This is the app version of [maxexq/sideglance](https://github.com/maxexq/sideglance), which runs from Python. The app needs no Python.

## Install

Download the installer for your system from [Releases](https://github.com/maxexq/sideglance-app/releases):

| System | File |
|---|---|
| Windows 10 or 11 | `Sideglance_x.y.z_x64-setup.exe` |
| macOS 11 or later (Apple silicon and Intel) | `Sideglance_x.y.z_universal.dmg` |

The builds are not signed yet, so each system warns you the first time:

- **Windows:** SmartScreen says "Windows protected your PC". Click **More info**, then **Run anyway**.
- **macOS:** after you drag Sideglance to Applications, the first open is blocked. Open **System Settings > Privacy & Security**, scroll down, and click **Open Anyway** next to Sideglance.

### Updates

Version 0.4.0 and later check for a newer version when they start and every 6 hours. When there
is one, the control page shows **Update and restart**, and the tray or menu bar item changes to
**Update to x.y.z and restart**. The app downloads the new version, checks its signature, installs
it, and opens again. To check now, click **Check for updates** under the display on the control
page, or in the tray menu.

If installing fails, the page links to the release on GitHub, and you can install it over the
current version.

## Set up

1. Pair the Ditoo Pro in your Bluetooth settings: **Settings > Bluetooth & devices** on Windows, **System Settings > Bluetooth** on macOS.
2. Close the Divoom phone app. The Ditoo talks to one program at a time.
3. Open Sideglance. The controls open in a window, and the app finds the Ditoo by itself. If it cannot tell which port is the Ditoo, pick one under **Change connection**.

Closing the window keeps Sideglance running. Open the controls again from the tray icon (Windows) or the menu bar icon (macOS). The menu also has **Start at sign-in**, on after the first run, and **Quit Sideglance**.

## What it does

**AI usage.** Claude and Codex usage on the display: percent used, then the time left until the limit resets, for the 5-hour window (D) and the 7-day window (W). Orange labels are Claude, green ones are Codex. It updates every 5 minutes. The app reads the logins that Claude Code and the Codex CLI keep on your computer. If one is not found, enter a token or the path to its credentials file under **Accounts and tokens**. Both usage endpoints are unofficial and can change.

**Notifications.** An icon over whatever is on the display:

| Event | When | Display |
|---|---|---|
| `claude-waiting` | Claude Code needs your answer | cursor blinks until you go back to Claude Code |
| `claude-finished` | Claude Code finished | stays for 10 seconds |
| `job-passed`, `job-failed` | your own scripts | stays for 10 seconds; failed blinks first |
| `timer-finished` | the timers, or your own scripts | blinks until dismissed |
| `battery-low` | this computer's battery is below 15% | red level blinks, then stays for 10 seconds |

On the **Notifications** tab, click **Connect Claude Code**. This adds hooks to `~/.claude/settings.json` and keeps the old file next to it as `settings.json.sideglance-backup`. Restart Claude Code afterwards. Each hook runs `curl`, which Windows 10 and later and macOS already have:

    curl -s -m 2 -X POST http://127.0.0.1:8765/api/hook/claude-waiting

Send any event from your own scripts the same way, for example `job-passed` at the end of a build.

**Weather.** A condition icon and the current temperature, updated every 15 minutes, from [Open-Meteo](https://open-meteo.com) (no key needed). The location is found from your internet address with ipapi.co, ipwho.is or ipinfo.io, tried in turn, or set a city or coordinates on the Weather tab. Choose Celsius or Fahrenheit, and optionally add the weather face after the usage faces. If a fetch fails, the last reading stays; before the first one, the face shows a cloud and `--°`.

**Timer.** A Pomodoro timer (focus 25 minutes, short break 5, long break 15 after the fourth session, all adjustable) with a ring of 60 pixels for the seconds and a dot per session, or a one-off countdown of up to 99 minutes. While a Pomodoro is on, its face covers the other faces. When a phase or the countdown ends, the timer-finished hourglass blinks.

**Computer.** How busy this computer is: CPU use on top (C), memory use below (R), updated every 3 seconds while it shows. On a laptop, the **Battery low** notification shows when the battery drops below 15% and it is not charging, at most every 30 minutes. The Ditoo Pro itself does not report its battery: its firmware ignores the battery query, as [ih8d8/divoom-ditoo-controller](https://github.com/ih8d8/divoom-ditoo-controller) also found.

**The device's own screens.** Clock, light, scoreboard, brightness, and a 16x16 drawing pad.

Not in the app yet, but in the Python version: image and GIF upload, and the hosted page.

Every face is drawn the same way as in the Python version: the tests compare them pixel for pixel.

### Moving from the Python version

Stop the Python server first, because both use port 8765. In the Python folder, run `python autostart.py uninstall`. The app's settings are separate, in the folder below, so set your location and timer lengths again in the app.

### The button on the device

Pressing the button on the Ditoo moves it through its own channels (clock, light, effects and so
on). Sideglance notices within about 2 seconds and stops sending, so it does not pull the screen
back. Press the button until the animation channel comes round again and Sideglance draws its
screen there. Choosing anything on the control page, a notification from Claude Code, or **Bring
Sideglance back** also returns the device to Sideglance.

### One display, one computer

The Ditoo Pro shows up in Bluetooth twice: an entry ending in **Audio** (the speaker) and one ending in **Light** (the display). Sideglance needs the **Light** one, and it can be connected to one computer at a time. To move it to another computer, disconnect it in the first computer's Bluetooth settings, turn the Ditoo off and on, then connect it on the second.

## Settings and data

Settings, including tokens you enter, are in `config.json`:

- Windows: `%APPDATA%\Sideglance\config.json`
- macOS: `~/Library/Application Support/Sideglance/config.json`

Tokens never leave your computer except to the usage endpoints, and the page never shows them.

## The local server

The app answers on `http://127.0.0.1:8765` while it runs. Only programs on your computer can use it, except `GET /api/scene`, which describes what should be on a display with no pixels, for screens that draw for themselves.

| Route | Use |
|---|---|
| `POST /api/hook/<event>` | show a notification; answers with no body (for hooks) |
| `POST /api/notify` `{"event": "<event>"}` | the same, from the page; `dismiss` removes it |
| `GET /api/status` | everything the page shows |
| `GET /api/scene` | usage, weather, timers and the notification as JSON |

## Build it yourself

You need Rust (stable), Node.js 22, and on Windows the Microsoft C++ Build Tools, on macOS the Xcode command line tools.

    cd ui && npm ci && npm run build && cd ..
    cargo install tauri-cli --version "^2"
    cargo tauri dev            # run it
    cargo tauri build          # make the installer for this system

Try the page and the server without a device or a window:

    SIDEGLANCE_FAKE=1 cargo run -p sideglance-core --example serve -- ui/dist

## How the code is organized

| Part | Folder | Job |
|---|---|---|
| Core | `crates/sideglance-core` | Everything without a window: the Divoom protocol, finding the Bluetooth port, the 16x16 renderer, notifications, AI usage, Claude Code hooks, and the local HTTP API. |
| App | `src-tauri` | The tray menu, start at sign-in, the window, and starting the core. |
| Page | `ui` | The control page (Svelte 5, Vite). The app serves it from `127.0.0.1:8765`, so the window, a browser and the hooks all use the same API. |

The core's tests compare its output with the Python version byte for byte (protocol) and pixel for pixel (faces and icons), using `crates/sideglance-core/tests/golden/python.json`. The Python version's encoder was checked on a real Ditoo Pro.

    cargo test -p sideglance-core

## Releases

1. Raise `version` in `Cargo.toml` and `src-tauri/tauri.conf.json`.
2. In GitHub, open **Actions > Release > Run workflow**, or push a tag (`git tag v0.4.0 && git push origin v0.4.0`).
   The workflow builds both installers, signs the update files, and attaches them with
   `latest.json` to a draft release.
3. Publish the draft. Installed apps find the new version through `latest.json` on the newest
   published release, so a draft reaches nobody.

The update files are signed with a key that is separate from Apple or Microsoft code signing. The
release workflow reads it from two repository secrets: `TAURI_SIGNING_PRIVATE_KEY` (the contents of
the `.key` file) and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. The public half is `plugins.updater.pubkey`
in `src-tauri/tauri.conf.json`. Keep a backup of the key and password: without them, installed apps
cannot be updated and people have to reinstall by hand.
