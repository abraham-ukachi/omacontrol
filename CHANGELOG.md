# Changelog

All notable changes to this project will be documented in this file. Versions follow [SemVer](https://semver.org), see [CONTRIBUTING.md](./CONTRIBUTING.md#git-commit-messages) for the commit guidelines.

## [0.3.4] - 2026-11-16

### Added
- :snowflake: Long-press **Test** on AC setup to open a warning and run a cancellable 675-candidate NEC sweep. The first code is the reported `00FF40BF`; OmaControl cannot detect IR responses, so the user confirms a match while the code is displayed.
- :earth_americas: Slint system-locale translation bundles for the AC setup flow in German, Spanish, French, Hindi, Italian, Japanese, Portuguese, Russian, Turkish and Simplified Chinese.

### Changed
- :memo: Added localization as a tracked 1.0 release requirement; translations fall back to English when the system locale is not bundled.

## [0.3.3] - 2026-11-15

### Added
- :computer: An x86_64 Linux release bundle with an app-launcher entry and icon.

### Changed
- :memo: Documented Omarchy/Linux install requirements and live-theme fallback behavior.
- :construction_worker: Android workflow now builds and publishes the Linux desktop bundle alongside the APK.

## [0.3.2] - 2026-11-15

### Added
- :art: Linux can follow Omarchy's active `colors.toml` theme live, with built-in themes still selectable.
- :dart: A tracked 1.0 roadmap for Omarchy/Linux, LAN-connected computer controls, AV devices, and release gates.

### Changed
- :lock: Tagged Android releases now require the permanent signing key instead of silently publishing an APK with a throwaway key.
- :memo: Documented Android release-key setup and the 1.0 scope.

## [0.3.1] - 2026-11-15

### Changed
- :point_up_2: Saved channels lose their **edit** button: **long-press** a channel to rename it (the name field opens with the keyboard), **swipe it up** to delete it, after an "Are you sure?" dialog. Drag the chips sideways to scroll them.
- :keyboard: The **Pronto code** field and its **Test code** key now ride just above the keyboard while you type, like the Channels screen, and a long placeholder is cut with an ellipsis instead of overflowing.
- :arrow_up: GitHub Actions bumped (checkout v7, setup-java v6, upload-artifact v7, action-gh-release v3), clearing the Node 20 warnings.

### Fixed
- :bug: Tuning a saved channel sent its digits too fast, so the Freebox showed `402` or `42` instead of `4002`. Digits now go out 350 ms apart.
- :zap: Images optimized (Imgbot, #1).

## [0.3.0-beta.1] - 2026-11-15

### Added
- :robot: **Android TV / Google TV** over the Android TV Remote protocol v2 (the Freebox Player TV Free 4K included): the app makes its own RSA certificate once, pairs with the 6-character code the TV shows (typos are caught before anything is sent), then drives keys, typing and app shortcuts on port 6466, answering the TV's pings to stay connected. Tested end to end against a fake TV.
- :mag: Android TVs are found with mDNS (`_androidtvremote2._tcp`) next to SSDP.
- :tv: **Saved channels**: type a number, **+ save**, name it (`4002` → `BFMTV`); one tap tunes it. Kept per TV, and kept through rescans.
- :gear: **Settings** with three tabs: **Theme** (moved out of the menu), **About** (version, Abraham Ukachi, GitHub, X, email, source, license) and **Feedback** (feedback or issue, optional reply email, title, message), sent straight to the maintainer's inbox through Web3Forms.
- :sparkles: An **ASCII radar** (with a bouncing bar and a blinking cursor) plays while scanning for devices.
- :point_up_2: Tap a device's name at the top of the Remote, Channels or AC screen to go back to the devices list.

## [0.2.4] - 2026-11-15

### Fixed
- :bug: The APK reports the crate version again: the workspace was still on 0.2.0 while the tags had moved on, so every build since 0.2.1 identified itself as 0.2.0 to Android.

### Changed
- :construction_worker: A release tag must match the workspace version: the Android workflow stops before building when `vX.Y.Z` and `Cargo.toml` disagree.

## [0.2.3] - 2026-11-14

### Added
- :snowflake: Two more air conditioner protocols: **Haier** (YRW02, 14-byte frame) and **Daikin** (35-byte, three-section state frame), next to Midea, Coolix, Gree, TCL, LG, Electra, Samsung and Kelvinator.

## [0.2.2] - 2026-11-14

### Added
- :snowflake: Four more air conditioner protocols: **LG**, **Electra** (Electra, AUX, AEG Chillflex, De'Longhi Pinguino, Frigidaire), **Samsung** and **Kelvinator**, next to Midea, Coolix, Gree and TCL.
- A pasted **Pronto code** for air conditioners no protocol reaches: the setup screen tests it, and the power key sends it.

### Changed
- AC setup sends each code four frames (on, on, off, on): a weak phone blaster gets several tries, a unit that is already running beeps on the off frame, and the code leaves it on.

### Fixed
- The device store is written atomically and the previous file is kept as `omacontrol.json.bak`; an unreadable file moves to `omacontrol.json.corrupt` and the backup is loaded instead of silently starting empty.

## [0.2.1] - 2026-11-14

### Fixed
- Adding a TV by IP no longer freezes the app: the brand probe runs on the worker thread, behind a "looking for the TV…" status.
- Clippy is clean with `--all-targets` (the Midea test vectors' digit grouping).

### Changed
- CI lints test code too (`cargo clippy --workspace --all-targets`), and `AGENTS.md` documents the same command.
- The docs match 0.2.0: **Devices** instead of *TVs*, six screens, the air conditioner module, and Android TV as 0.3.0.

## [0.2.0-beta.1] - 2026-11-14

### Added
- :snowflake: Air conditioners over IR: Midea, Coolix, Gree and TCL protocols (most rebadged portables and splits, Cecotec included).
- A setup that tries each protocol until the AC beeps (phones can send IR but not receive it).
- The AC remote: power, mode, fan, − 22° +, sleep and swing, with the state saved between launches.
- The OK key launcher icon (adaptive on Android 8+).

### Changed
- The TVs screen is now **Devices**, and **Add TV** is **Add**.

## [0.1.0-beta.2] - 2026-11-14

### Added
- A faint haptic tick on every key, rocker and menu button.
- A short accent-colored pulse on each tap while an IR device is in use.
- Six launcher icon concepts in `design/icons/`.

### Fixed
- The "type to TV" row and GO now ride just above the on-screen keyboard while typing.

## 0.1.0 (2026-11-14)

* :tada: First version of **OmaControl**: a Rust + Slint remote for smart TVs, with Omarchy's look & feel (Omacalc is the reference)
* :tv: Three screens: **TVs** on your network, the **Remote** (D-pad, OK, back, home, menu, VOL / CH rockers, media keys) and **Channels & apps** (number pad, streaming shortcuts, type to the TV)
* :mag: TV discovery with SSDP (Samsung, LG, Roku, Android TV), or add a TV by IP
* :satellite: Protocols: Samsung Tizen (secure WebSocket, token pairing), LG webOS (SSAP + pointer socket, client key pairing), Roku (ECP)
* :red_circle: Infrared engine (NEC, Samsung32, Sony SIRC, Pronto hex) and the phone's IR blaster (`ConsumerIrManager`): Samsung, LG and Sony TVs over IR
* :art: All 23 Omarchy themes (generated from `omacom/omarchy`), colors mixed the Omacalc way, iA Writer Mono
* :construction_worker: CI (tests, check, clippy) and an Android workflow that builds the APK (attached to every `v*` release)
