# Changelog

All notable changes to this project will be documented in this file. Versions follow [SemVer](https://semver.org), see [CONTRIBUTING.md](./CONTRIBUTING.md#git-commit-messages) for the commit guidelines.

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
