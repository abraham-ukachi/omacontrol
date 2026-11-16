//! # OmaControl
//!
//! The app: the Slint UI (`ui/*.slint`) wired to `omacontrol-core`.
//! Everything that talks to a TV runs on a worker thread, so the UI never waits on the network.

use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use std::cell::{Cell, RefCell};

use omacontrol_core::feedback::{Feedback as FeedbackForm, Kind as FeedbackKind};
use omacontrol_core::ir::Pattern;
use omacontrol_core::tv::androidtv::{Identity, Pairing};
use omacontrol_core::{connect, connect_android_tv, discover, AcChange, AcProtocol, App, Brand, Device, Error, IrTransmitter, Key, Remote, Store, Theme as OmaTheme, Transport};
use slint::{Color, ModelRc, SharedString, VecModel};

slint::include_modules!();

#[cfg(target_os = "android")]
mod android_ir;
#[cfg(target_os = "android")]
mod android_url;
#[cfg(target_os = "android")]
mod haptics;

/// Opens a link (About: GitHub, X, mail) in the app that handles it.
fn open_url(url: &str) {
    #[cfg(target_os = "android")]
    {
        android_url::open(url);
    }
    #[cfg(not(target_os = "android"))]
    {
        let opener = if cfg!(target_os = "macos") { "open" } else if cfg!(windows) { "explorer" } else { "xdg-open" };
        let _ = std::process::Command::new(opener).arg(url).spawn();
    }
}

/// "0.3.0 · Android", for the feedback email.
fn version_line() -> String {
    let os = if cfg!(target_os = "android") { "Android" } else { std::env::consts::OS };
    format!("{} · {os}", env!("CARGO_PKG_VERSION"))
}

/// A faint tick under the finger (Android only; desktops have nothing to vibrate).
fn haptic_tick() {
    #[cfg(target_os = "android")]
    haptics::tick();
}

/// The phone's IR blaster (Android only).
fn blaster() -> Option<Arc<dyn IrTransmitter>> {
    #[cfg(target_os = "android")]
    {
        let ir = android_ir::AndroidIr;
        if ir.has_emitter() {
            return Some(Arc::new(ir));
        }
    }
    None
}

/// What the worker thread is asked to do.
/// The pause between digits when a saved channel is tuned.
const TUNE_DIGIT_GAP_MS: u64 = 350;
/// Gives the user time to observe the AC before the next NEC candidate is sent.
const NEC_SCAN_STEP_GAP_MS: u64 = 350;

enum Job {
    Scan,
    /// A TV added by IP: find its brand, then connect.
    Probe(String),
    Connect(Device),
    Press(Key),
    Text(String),
    Launch(App),
    /// IR frames for the air conditioner, sent one after another.
    Ac(Vec<Pattern>),
    /// A bounded NEC discovery sweep; cancellation is checked between frames.
    AcScan { candidates: Vec<(u8, u8)>, cancel: Arc<AtomicBool> },
    /// A saved channel: its digits, one after another.
    Tune(String),
    /// The code an Android TV shows while pairing.
    Pair(String),
    CancelPair,
    /// Settings › Feedback, sent to the maintainer's inbox.
    Feedback(FeedbackForm),
}

/// Shows an air conditioner on the AC page.
fn show_ac(ui: &AppWindow, d: &Device) {
    let s = d.ac_state;
    ui.set_current_is_ac(true);
    ui.set_ac_name(d.name.clone().into());
    ui.set_ac_custom_code(d.ac_code.is_some());
    ui.set_ac_status(match (&d.ac_protocol, d.ac_code.is_some()) {
        (_, true) => d
            .ac_code
            .as_deref()
            .filter(|code| code.starts_with("NEC "))
            .map(|code| format!("{code} · IR").into())
            .unwrap_or_else(|| "code · IR".into()),
        (Some(p), false) => format!("{} · IR", p.label()).into(),
        (None, false) => "not set up yet".into(),
    });
    ui.set_ac_power(s.power);
    ui.set_ac_temp(s.temp as i32);
    ui.set_ac_mode(s.mode.label().into());
    ui.set_ac_fan(s.fan.label().into());
    ui.set_ac_swing(s.swing);
    ui.set_ac_sleep(s.sleep);
    ui.global::<Feedback>().set_ir(true);
}

fn show_ac_setup(ui: &AppWindow, index: usize, tested: bool) {
    ui.set_ac_setup_index(index as i32);
    ui.set_ac_setup_count(AcProtocol::ALL.len() as i32);
    ui.set_ac_setup_protocol(AcProtocol::ALL[index].label().into());
    ui.set_ac_setup_tested(tested);
}

fn color(rgb: omacontrol_core::theme::Rgb) -> Color {
    Color::from_argb_encoded(rgb.argb())
}

/// Paints the whole UI with an Omarchy theme.
fn apply_theme(ui: &AppWindow, theme: &OmaTheme) {
    let g = ui.global::<Theme>();
    let (bg, fg) = (theme.background, theme.foreground);
    g.set_name(theme.name.clone().into());
    g.set_bg(color(bg));
    g.set_fg(color(fg));
    g.set_accent(color(theme.accent));
    g.set_key(color(theme.key()));
    g.set_key_pressed(color(bg.mix(fg, 0.14)));
    g.set_key_border(color(theme.key_border()));
    g.set_operator(color(theme.operator()));
    g.set_operator_pressed(color(bg.mix(fg, 0.25)));
    g.set_primary_pressed(color(fg.mix(bg, 0.22)));
    g.set_divider(color(theme.divider()));
    g.set_muted(color(theme.muted()));
}

#[cfg(target_os = "linux")]
const OMARCHY_THEME_NAME: &str = "Omarchy (live)";

#[cfg(target_os = "linux")]
fn omarchy_colors_path() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state/omarchy/current/theme/colors.toml"))
}

#[cfg(target_os = "linux")]
fn load_omarchy_theme() -> Result<(OmaTheme, String), String> {
    let path = omarchy_colors_path().ok_or_else(|| "HOME is not set".to_string())?;
    let source = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let theme = OmaTheme::from_omarchy_colors(OMARCHY_THEME_NAME, &source).map_err(|e| e.to_string())?;
    Ok((theme, source))
}

#[cfg(target_os = "linux")]
fn selected_theme(name: &str) -> Result<OmaTheme, String> {
    if name == OMARCHY_THEME_NAME {
        load_omarchy_theme().map(|(theme, _)| theme)
    } else {
        Ok(OmaTheme::named(name))
    }
}

#[cfg(not(target_os = "linux"))]
fn selected_theme(name: &str) -> Result<OmaTheme, String> {
    Ok(OmaTheme::named(name))
}

#[cfg(target_os = "linux")]
fn watch_omarchy_theme(weak: slint::Weak<AppWindow>, store: Arc<Mutex<Store>>, initial_source: String) -> slint::Timer {
    let timer = slint::Timer::default();
    let mut last_observed = Some(initial_source);
    timer.start(slint::TimerMode::Repeated, Duration::from_secs(1), move || {
        let selected = store.lock().unwrap().theme.clone();
        if selected.as_deref().is_some_and(|name| name != OMARCHY_THEME_NAME) {
            return;
        }

        match load_omarchy_theme() {
            Ok((theme, source)) if last_observed.as_deref() != Some(source.as_str()) => {
                last_observed = Some(source);
                if let Some(ui) = weak.upgrade() {
                    apply_theme(&ui, &theme);
                }
            }
            Ok(_) => {}
            Err(error) => {
                let marker = format!("\0{error}");
                if last_observed.as_deref() != Some(marker.as_str()) {
                    log::warn!("could not read Omarchy theme: {error}");
                    last_observed = Some(marker);
                }
            }
        }
    });
    timer
}

/// The saved channels of the current TV.
fn channel_rows(store: &Store) -> ModelRc<ChannelRow> {
    let rows: Vec<ChannelRow> = store
        .current
        .as_deref()
        .and_then(|ip| store.device(ip))
        .map(|d| d.channels.iter().map(|c| ChannelRow { number: c.number.clone().into(), name: c.name.clone().into() }).collect())
        .unwrap_or_default();
    ModelRc::from(Rc::new(VecModel::from(rows)))
}

fn rows(store: &Store) -> ModelRc<DeviceRow> {
    let rows: Vec<DeviceRow> = store
        .devices
        .iter()
        .map(|d| DeviceRow {
            name: d.name.clone().into(),
            meta: if d.is_ac() { match (&d.ac_protocol, d.ac_code.is_some()) { (_, true) => "code · IR".into(), (Some(p), false) => format!("{} · IR", p.label()).into(), (None, false) => "IR · tap to set up".into() } } else if d.transport == Transport::Infrared { format!("{} · IR", d.brand.label()).into() } else if d.model.is_empty() { d.brand.label().into() } else { format!("{} · {}", d.model, d.brand.label()).into() },
            ip: if d.transport == Transport::Infrared { "IR".into() } else { d.ip.rsplit('.').next().unwrap_or_default().into() },
            current: store.current.as_deref() == Some(d.ip.as_str()),
            ac: d.is_ac(),
        })
        .collect();
    ModelRc::from(Rc::new(VecModel::from(rows)))
}

/// Starts the app. `inset_top` / `inset_bottom` keep the UI clear of the status and navigation bars.
pub fn run(data_dir: PathBuf, inset_top: f32, inset_bottom: f32) {
    let ui = AppWindow::new().expect("the window opens");
    let store = Arc::new(Mutex::new(Store::load(&data_dir)));

    #[cfg(target_os = "linux")]
    let live_theme = omarchy_colors_path()
        .filter(|path| path.is_file())
        .and_then(|_| match load_omarchy_theme() {
            Ok(theme) => Some(theme),
            Err(error) => {
                log::warn!("could not load Omarchy theme: {error}");
                None
            }
        });

    {
        let s = store.lock().unwrap();
        #[cfg(target_os = "linux")]
        let theme = if s.theme.as_deref() == Some(OMARCHY_THEME_NAME) || (s.theme.is_none() && live_theme.is_some()) {
            live_theme.as_ref().map(|(theme, _)| theme.clone()).unwrap_or_else(|| OmaTheme::named("Tokyo Night"))
        } else {
            OmaTheme::named(s.theme.as_deref().unwrap_or("Tokyo Night"))
        };
        #[cfg(not(target_os = "linux"))]
        let theme = OmaTheme::named(s.theme.as_deref().unwrap_or("Tokyo Night"));
        apply_theme(&ui, &theme);
        ui.set_devices(rows(&s));
        ui.set_channels(channel_rows(&s));
        if let Some(d) = s.current.as_deref().and_then(|ip| s.device(ip)) {
            if d.is_ac() {
                show_ac(&ui, d);
                ui.set_page(if d.ac_protocol.is_some() { 4 } else { 0 });
            } else {
                ui.set_tv_name(d.name.clone().into());
                ui.set_page(1);
            }
        }
    }
    ui.set_inset_top(inset_top);
    ui.set_inset_bottom(inset_bottom);
    ui.set_version(env!("CARGO_PKG_VERSION").into());
    let apps: Vec<SharedString> = App::ALL.iter().map(|a| a.label().into()).collect();
    ui.set_apps(ModelRc::from(Rc::new(VecModel::from(apps))));
    let themes: Vec<SharedString> = OmaTheme::all().into_iter().map(|t| t.name.into()).collect();
    #[cfg(target_os = "linux")]
    let themes = if live_theme.is_some() {
        let mut themes = themes;
        themes.insert(0, OMARCHY_THEME_NAME.into());
        themes
    } else {
        themes
    };
    ui.set_themes(ModelRc::from(Rc::new(VecModel::from(themes))));

    let ir = blaster();
    ui.set_has_ir(ir.is_some());
    let jobs = spawn_worker(ui.as_weak(), store.clone(), ir);

    // Reconnect to the last TV, and look for others.
    if let Some(d) = { let s = store.lock().unwrap(); s.current.as_deref().and_then(|ip| s.device(ip)).filter(|d| !d.is_ac()).cloned() } {
        let _ = jobs.send(Job::Connect(d));
    }
    let _ = jobs.send(Job::Scan);

    let tx = jobs.clone();
    ui.on_rescan(move || { let _ = tx.send(Job::Scan); });

    let (tx, st, weak) = (jobs.clone(), store.clone(), ui.as_weak());
    ui.on_pick(move |i| {
        let device = st.lock().unwrap().devices.get(i as usize).cloned();
        if let (Some(d), Some(ui)) = (device.clone(), weak.upgrade()) {
            if d.is_ac() {
                let mut s = st.lock().unwrap();
                s.current = Some(d.ip.clone());
                let _ = s.save();
                ui.set_devices(rows(&s));
                show_ac(&ui, &d);
                if d.ac_protocol.is_some() {
                    ui.set_page(4);
                } else {
                    show_ac_setup(&ui, 0, false);
                    ui.set_page(5);
                }
                return;
            }
            ui.set_current_is_ac(false);
            ui.set_pairing(false);
            ui.set_tv_name(d.name.clone().into());
            ui.set_page(1);
            {
                // Show this TV's saved channels right away (the connection catches up).
                let mut s = st.lock().unwrap();
                s.current = Some(d.ip.clone());
                ui.set_channels(channel_rows(&s));
                ui.set_devices(rows(&s));
                let _ = s.save();
            }
            let _ = tx.send(Job::Connect(d));
        }
    });

    let tx = jobs.clone();
    ui.on_add_ip(move |ip| {
        let ip = ip.trim().to_string();
        if ip.is_empty() {
            return;
        }
        // The worker probes the brands (a few TCP connects) and connects, so the UI never waits.
        let _ = tx.send(Job::Probe(ip));
    });

    let (tx, st, weak) = (jobs.clone(), store.clone(), ui.as_weak());
    ui.on_add_ir(move |brand| {
        let brand = match brand.as_str() {
            "Samsung" => Brand::Samsung,
            "LG" => Brand::Lg,
            _ => Brand::Sony,
        };
        let d = Device::infrared_tv(brand);
        let mut s = st.lock().unwrap();
        s.upsert(d.clone());
        let _ = s.save();
        if let Some(ui) = weak.upgrade() {
            ui.set_devices(rows(&s));
            ui.set_current_is_ac(false);
            ui.set_tv_name(d.name.clone().into());
            ui.set_page(1);
        }
        let _ = tx.send(Job::Connect(d));
    });

    // Air conditioners.
    let setup_index = Rc::new(Cell::new(0usize));
    let (st, weak, idx) = (store.clone(), ui.as_weak(), setup_index.clone());
    ui.on_add_ac(move || {
        let mut s = st.lock().unwrap();
        let n = s.devices.iter().filter(|d| d.is_ac()).count() + 1;
        let d = Device::infrared_ac(n);
        s.upsert(d.clone());
        s.current = Some(d.ip.clone());
        let _ = s.save();
        if let Some(ui) = weak.upgrade() {
            ui.set_devices(rows(&s));
            show_ac(&ui, &d);
            idx.set(0);
            show_ac_setup(&ui, 0, false);
            ui.set_page(5);
        }
    });

    let pasted = Rc::new(RefCell::new(String::new()));
    let typed = pasted.clone();
    ui.on_ac_code_changed(move |text| { *typed.borrow_mut() = text.to_string(); });

    let scan_cancel = Rc::new(RefCell::new(None::<Arc<AtomicBool>>));
    let (tx, st, weak, idx, pasted, scan_cancel_state) = (
        jobs.clone(),
        store.clone(),
        ui.as_weak(),
        setup_index.clone(),
        pasted,
        scan_cancel.clone(),
    );
    ui.on_ac_setup(move |step| {
        let Some(ui) = weak.upgrade() else { return };
        let protocol = AcProtocol::ALL[idx.get()];
        match step.as_str() {
            "test" => {
                let _ = tx.send(Job::Ac(protocol.test_frames()));
                show_ac_setup(&ui, idx.get(), true);
            }
            "next" => {
                idx.set((idx.get() + 1) % AcProtocol::ALL.len());
                show_ac_setup(&ui, idx.get(), false);
            }
            "code" => match omacontrol_core::ac::custom_frames(&pasted.borrow().clone()) {
                Ok(frames) => {
                    let _ = tx.send(Job::Ac(frames));
                    show_ac_setup(&ui, idx.get(), true);
                }
                Err(e) => ui.set_ac_setup_protocol(format!("{e}").into()),
            },
            "scan-confirm" => {
                let cancel = Arc::new(AtomicBool::new(false));
                *scan_cancel_state.borrow_mut() = Some(cancel.clone());
                ui.set_ac_scan_active(true);
                ui.set_ac_scan_finished(false);
                ui.set_ac_scan_cancelled(false);
                ui.set_ac_scan_count(0);
                let candidates = omacontrol_core::ac::nec_autotest_candidates();
                ui.set_ac_scan_total(candidates.len() as i32);
                ui.set_ac_scan_code("".into());
                ui.set_ac_scan_result("".into());
                if let Err(e) = tx.send(Job::AcScan {
                    candidates,
                    cancel,
                }) {
                    *scan_cancel_state.borrow_mut() = None;
                    ui.set_ac_scan_active(false);
                    ui.set_ac_scan_finished(true);
                    ui.set_ac_scan_cancelled(false);
                    ui.set_ac_scan_result(format!("Could not start the scan: {e}").into());
                }
            }
            "scan-found" => {
                let code = ui.get_ac_scan_code().to_string();
                if code.is_empty() {
                    return;
                }
                if let Some(cancel) = scan_cancel_state.borrow_mut().take() {
                    cancel.store(true, Ordering::Relaxed);
                }
                let mut s = st.lock().unwrap();
                let Some(ip) = s.current.clone() else { return };
                if let Some(d) = s.devices.iter_mut().find(|d| d.ip == ip && d.is_ac()) {
                    d.ac_protocol = None;
                    d.ac_code = Some(code);
                    d.ac_state = omacontrol_core::ac::TEST_STATE;
                    let d = d.clone();
                    let _ = s.save();
                    ui.set_devices(rows(&s));
                    show_ac(&ui, &d);
                    ui.set_ac_scan_active(false);
                    ui.set_ac_scan_finished(false);
                    ui.set_ac_scan_cancelled(false);
                    ui.set_page(4);
                }
            }
            "scan-dismiss" => {
                *scan_cancel_state.borrow_mut() = None;
                ui.set_ac_scan_finished(false);
                ui.set_ac_scan_result("".into());
            }
            _ => {
                let mut s = st.lock().unwrap();
                let Some(ip) = s.current.clone() else { return };
                let code = pasted.borrow().clone();
                let pasted_code = (!code.is_empty() && omacontrol_core::ac::custom_frames(&code).is_ok()).then_some(code);
                if let Some(d) = s.devices.iter_mut().find(|d| d.ip == ip) {
                    d.ac_protocol = if pasted_code.is_some() { None } else { Some(protocol) };
                    d.ac_code = pasted_code;
                    d.ac_state = omacontrol_core::ac::TEST_STATE;
                    let d = d.clone();
                    let _ = s.save();
                    ui.set_devices(rows(&s));
                    show_ac(&ui, &d);
                    ui.set_page(4);
                }
            }
        }
    });
    let scan_cancel = scan_cancel.clone();
    ui.on_ac_scan_cancel(move || {
        if let Some(cancel) = scan_cancel.borrow().as_ref() {
            cancel.store(true, Ordering::Relaxed);
        }
    });

    let (tx, st, weak) = (jobs.clone(), store.clone(), ui.as_weak());
    ui.on_ac_action(move |action| {
        let Some(ui) = weak.upgrade() else { return };
        let mut s = st.lock().unwrap();
        let Some(ip) = s.current.clone() else { return };
        let Some(d) = s.devices.iter_mut().find(|d| d.ip == ip && d.is_ac()) else { return };
        if let Some(code) = d.ac_code.clone() {
            // A pasted code is one command (usually power); the state keys have nothing to send.
            if action.as_str() != "power" {
                return;
            }
            d.ac_state.power = !d.ac_state.power;
            let Ok(frames) = omacontrol_core::ac::custom_frames(&code) else { return };
            let d = d.clone();
            let _ = s.save();
            drop(s);
            show_ac(&ui, &d);
            let _ = tx.send(Job::Ac(frames));
            return;
        }
        let Some(protocol) = d.ac_protocol else { return };
        let state = &mut d.ac_state;
        let change = match action.as_str() {
            "power" => { state.power = !state.power; AcChange::State }
            "mode" => { state.mode = state.mode.next(); AcChange::State }
            "fan" => { state.fan = state.fan.next(); AcChange::State }
            "warmer" => { state.warmer(); AcChange::State }
            "cooler" => { state.cooler(); AcChange::State }
            "swing" => { state.swing = !state.swing; AcChange::Swing }
            "sleep" => { state.sleep = !state.sleep; AcChange::Sleep }
            _ => return,
        };
        let frames = protocol.frames(state, change);
        let d = d.clone();
        let _ = s.save();
        drop(s);
        show_ac(&ui, &d);
        let _ = tx.send(Job::Ac(frames));
    });

    ui.global::<Feedback>().on_tap(haptic_tick);

    let tx = jobs.clone();
    ui.on_press(move |id| {
        if let Some(k) = Key::from_id(&id) {
            let _ = tx.send(Job::Press(k));
        }
    });
    let tx = jobs.clone();
    ui.on_launch(move |label| {
        if let Some(a) = App::from_label(&label) {
            let _ = tx.send(Job::Launch(a));
        }
    });
    let tx = jobs.clone();
    ui.on_send_text(move |t| { let _ = tx.send(Job::Text(t.to_string())); });

    // Saved channels.
    let tx = jobs.clone();
    ui.on_tune(move |number| { let _ = tx.send(Job::Tune(number.to_string())); });
    let (st, weak) = (store.clone(), ui.as_weak());
    ui.on_save_channel(move |number, name| {
        let mut s = st.lock().unwrap();
        let Some(ip) = s.current.clone() else { return };
        let changed = s.devices.iter_mut().find(|d| d.ip == ip).is_some_and(|d| d.save_channel(&number, &name));
        if changed {
            let _ = s.save();
            if let Some(ui) = weak.upgrade() {
                ui.set_channels(channel_rows(&s));
            }
        }
    });
    let (st, weak) = (store.clone(), ui.as_weak());
    ui.on_remove_channel(move |number| {
        let mut s = st.lock().unwrap();
        let Some(ip) = s.current.clone() else { return };
        if s.devices.iter_mut().find(|d| d.ip == ip).is_some_and(|d| d.remove_channel(&number)) {
            let _ = s.save();
            if let Some(ui) = weak.upgrade() {
                ui.set_channels(channel_rows(&s));
            }
        }
    });

    // Android TV pairing.
    let tx = jobs.clone();
    ui.on_pair(move |code| { let _ = tx.send(Job::Pair(code.to_string())); });
    let tx = jobs.clone();
    ui.on_cancel_pair(move || { let _ = tx.send(Job::CancelPair); });

    // Settings.
    ui.on_open_url(|url| open_url(&url));
    let (tx, weak) = (jobs.clone(), ui.as_weak());
    ui.on_send_feedback(move || {
        let Some(ui) = weak.upgrade() else { return };
        let form = FeedbackForm {
            kind: FeedbackKind::from_label(&ui.get_fb_kind()),
            email: ui.get_fb_email().to_string(),
            title: ui.get_fb_title().to_string(),
            message: ui.get_fb_message().to_string(),
            version: version_line(),
        };
        if let Err(why) = form.validate() {
            ui.set_fb_status(why.into());
            return;
        }
        ui.set_fb_sending(true);
        ui.set_fb_status("sending…".into());
        let _ = tx.send(Job::Feedback(form));
    });

    let (st, weak) = (store.clone(), ui.as_weak());
    ui.on_pick_theme(move |name| {
        let theme = match selected_theme(&name) {
            Ok(theme) => theme,
            Err(error) => {
                log::warn!("could not select theme {name}: {error}");
                return;
            }
        };
        if let Some(ui) = weak.upgrade() {
            apply_theme(&ui, &theme);
        }
        let mut s = st.lock().unwrap();
        s.theme = Some(name.to_string());
        let _ = s.save();
    });

    #[cfg(target_os = "linux")]
    let _omarchy_theme_timer = live_theme
        .as_ref()
        .map(|(_, source)| watch_omarchy_theme(ui.as_weak(), store.clone(), source.clone()));

    ui.run().expect("the event loop runs");
}

/// Guesses the brand of a TV added by IP from the ports it answers on.
fn probe_brand(ip: &str) -> Brand {
    use std::net::{SocketAddr, TcpStream};
    let open = |port: u16| {
        format!("{ip}:{port}")
            .parse::<SocketAddr>()
            .map(|a| TcpStream::connect_timeout(&a, Duration::from_millis(600)).is_ok())
            .unwrap_or(false)
    };
    if open(8060) {
        Brand::Roku
    } else if open(8002) {
        Brand::Samsung
    } else if open(3001) || open(3000) {
        Brand::Lg
    } else if open(6466) {
        Brand::AndroidTv
    } else {
        Brand::Unknown
    }
}

/// The worker thread: owns the connection to the current TV.
fn spawn_worker(weak: slint::Weak<AppWindow>, store: Arc<Mutex<Store>>, ir: Option<Arc<dyn IrTransmitter>>) -> Sender<Job> {
    let (tx, rx) = mpsc::channel::<Job>();
    let status = move |weak: &slint::Weak<AppWindow>, text: String, online: bool| {
        let _ = weak.upgrade_in_event_loop(move |ui| {
            ui.set_tv_status(text.into());
            ui.set_online(online);
        });
    };
    thread::spawn(move || {
        let mut remote: Option<Box<dyn Remote>> = None;
        let mut label = String::new();
        // An Android TV showing its pairing code, waiting for the user to type it.
        let mut pending: Option<(Device, Pairing)> = None;
        for job in rx {
            match job {
                Job::Scan => {
                    let _ = weak.upgrade_in_event_loop(|ui| ui.set_scanning(true));
                    let found = discover(Duration::from_secs(3)).unwrap_or_default();
                    let mut s = store.lock().unwrap();
                    for d in found {
                        s.upsert(d);
                    }
                    let _ = s.save();
                    let snapshot = s.clone();
                    drop(s);
                    let _ = weak.upgrade_in_event_loop(move |ui| {
                        ui.set_devices(rows(&snapshot));
                        ui.set_scanning(false);
                    });
                }
                Job::Connect(device) => {
                    pending = None;
                    let (r, l) = connect_device(&weak, &store, &ir, &device, &status, &mut pending);
                    remote = r;
                    label = l;
                }
                Job::Pair(code) => {
                    let Some((device, pairing)) = pending.take() else { continue };
                    // A typo is caught before anything is sent, and the TV keeps its code up.
                    if let Err(e) = pairing.check(&code) {
                        status(&weak, e.to_string(), false);
                        pending = Some((device, pairing));
                        continue;
                    }
                    status(&weak, "pairing…".into(), false);
                    match pairing.finish(&code) {
                        Ok(()) => {
                            let mut paired = device.clone();
                            paired.token = Some("paired".into());
                            {
                                let mut s = store.lock().unwrap();
                                s.upsert(paired.clone());
                                let _ = s.save();
                            }
                            let _ = weak.upgrade_in_event_loop(|ui| ui.set_pairing(false));
                            let (r, l) = connect_device(&weak, &store, &ir, &paired, &status, &mut pending);
                            remote = r;
                            label = l;
                        }
                        Err(e) => {
                            let _ = weak.upgrade_in_event_loop(|ui| ui.set_pairing(false));
                            status(&weak, format!("{e} · tap the name to try again"), false);
                        }
                    }
                }
                Job::CancelPair => {
                    pending = None;
                    let _ = weak.upgrade_in_event_loop(|ui| ui.set_pairing(false));
                    status(&weak, "not paired".into(), false);
                }
                Job::Tune(number) => {
                    let Some(r) = remote.as_mut() else {
                        status(&weak, "pick a TV first".into(), false);
                        continue;
                    };
                    let mut result = Ok(());
                    for c in number.chars() {
                        let key = if c == '-' { Key::Dash } else if let Some(d) = c.to_digit(10) { Key::Digit(d as u8) } else { continue };
                        result = r.press(key);
                        if result.is_err() {
                            break;
                        }
                        // TVs and set-top boxes drop digits sent back to back (4002 showed up as 402 or 42
                        // on a Freebox), so give each one a beat, like a thumb on a remote would.
                        thread::sleep(Duration::from_millis(TUNE_DIGIT_GAP_MS));
                    }
                    report(&weak, &label, result, &status);
                }
                Job::Feedback(form) => {
                    let result = form.send();
                    let _ = weak.upgrade_in_event_loop(move |ui| {
                        ui.set_fb_sending(false);
                        match result {
                            Ok(()) => {
                                ui.set_fb_status("sent. thank you!".into());
                                ui.set_fb_title("".into());
                                ui.set_fb_message("".into());
                            }
                            Err(e) => ui.set_fb_status(format!("not sent: {e}").into()),
                        }
                    });
                }
                Job::Probe(ip) => {
                    // A TV added by hand: try the brands one by one, Roku first (it needs no pairing).
                    let name = format!("TV {ip}");
                    let shown = name.clone();
                    let _ = weak.upgrade_in_event_loop(move |ui| {
                        ui.set_current_is_ac(false);
                        ui.set_tv_name(shown.into());
                        ui.set_tv_status("looking for the TV…".into());
                        ui.set_online(false);
                        ui.set_page(1);
                    });
                    let brand = probe_brand(&ip);
                    let device = Device::new(&name, "", brand, &ip);
                    {
                        let mut s = store.lock().unwrap();
                        s.upsert(device.clone());
                        let _ = s.save();
                        let snapshot = s.clone();
                        drop(s);
                        let _ = weak.upgrade_in_event_loop(move |ui| ui.set_devices(rows(&snapshot)));
                    }
                    let (r, l) = connect_device(&weak, &store, &ir, &device, &status, &mut pending);
                    remote = r;
                    label = l;
                }
                Job::Ac(frames) => {
                    let result = match &ir {
                        None => Err("this phone has no IR blaster".to_string()),
                        Some(ir) => frames.iter().enumerate().try_for_each(|(i, f)| {
                            if i > 0 {
                                thread::sleep(Duration::from_millis(80));
                            }
                            ir.transmit(f).map_err(|e| e.to_string())
                        }),
                    };
                    if let Err(e) = result {
                        let _ = weak.upgrade_in_event_loop(move |ui| ui.set_ac_status(e.into()));
                    }
                }
                Job::AcScan { candidates, cancel } => {
                    let mut sent = 0usize;
                    let mut failure = None;
                    match &ir {
                        None => failure = Some("this phone has no IR blaster".to_string()),
                        Some(ir) if !ir.has_emitter() => {
                            failure = Some("this phone has no IR blaster".to_string())
                        }
                        Some(ir) => {
                            for (address, command) in candidates.iter().copied() {
                                if cancel.load(Ordering::Relaxed) {
                                    break;
                                }
                                if let Err(e) =
                                    ir.transmit(&omacontrol_core::ir::nec(address, command))
                                {
                                    failure = Some(e.to_string());
                                    break;
                                }
                                sent += 1;
                                let code = format!("NEC {:08X}", omacontrol_core::ir::nec_code(address, command));
                                let count = sent as i32;
                                let _ = weak.upgrade_in_event_loop(move |ui| {
                                    ui.set_ac_scan_count(count);
                                    ui.set_ac_scan_code(code.into());
                                });
                                if sent < candidates.len() {
                                    thread::sleep(Duration::from_millis(NEC_SCAN_STEP_GAP_MS));
                                }
                            }
                        }
                    }
                    let stopped = failure.is_none() && cancel.load(Ordering::Relaxed);
                    let result = failure.unwrap_or_default();
                    let _ = weak.upgrade_in_event_loop(move |ui| {
                        ui.set_ac_scan_active(false);
                        ui.set_ac_scan_finished(true);
                        ui.set_ac_scan_cancelled(stopped);
                        ui.set_ac_scan_result(result.into());
                    });
                }
                Job::Press(_) | Job::Text(_) | Job::Launch(_) if remote.is_none() => {
                    status(&weak, "pick a TV first".into(), false);
                }
                Job::Press(k) => report(&weak, &label, remote.as_mut().unwrap().press(k), &status),
                Job::Text(t) => report(&weak, &label, remote.as_mut().unwrap().type_text(&t), &status),
                Job::Launch(a) => report(&weak, &label, remote.as_mut().unwrap().launch(a), &status),
            }
        }
    });
    tx
}

/// Connects to a device, saves its pairing token and refreshes the list and the header.
fn connect_device(
    weak: &slint::Weak<AppWindow>,
    store: &Arc<Mutex<Store>>,
    ir: &Option<Arc<dyn IrTransmitter>>,
    device: &Device,
    status: &impl Fn(&slint::Weak<AppWindow>, String, bool),
    pending: &mut Option<(Device, Pairing)>,
) -> (Option<Box<dyn Remote>>, String) {
    let label = if device.transport == Transport::Infrared { format!("{} · IR", device.brand.label()) } else { device.brand.label().to_string() };
    status(weak, format!("connecting · {label}"), false);
    if device.transport == Transport::Network && matches!(device.brand, Brand::Samsung | Brand::Lg) && device.token.is_none() {
        status(weak, "say yes on the TV…".into(), false);
    }
    let android_tv = device.transport == Transport::Network && device.brand == Brand::AndroidTv;
    let connected = if android_tv {
        match android_identity(store) {
            Ok(id) => match connect_android_tv(&device.ip, &id) {
                Err(Error::NotPaired) => {
                    // The TV doesn't know our certificate: ask it for a code.
                    status(weak, "asking the TV for a code…".into(), false);
                    match Pairing::start(&device.ip, &id, "OmaControl") {
                        Ok(p) => {
                            *pending = Some((device.clone(), p));
                            let _ = weak.upgrade_in_event_loop(|ui| ui.set_pairing(true));
                            status(weak, "type the code on the TV".into(), false);
                            return (None, label);
                        }
                        Err(e) => Err(e),
                    }
                }
                other => other,
            },
            Err(e) => Err(e),
        }
    } else {
        connect(device, ir.clone())
    };
    match connected {
        Ok(r) => {
            let mut s = store.lock().unwrap();
            let mut d = device.clone();
            d.token = r.token().or(d.token);
            s.upsert(d);
            s.current = Some(device.ip.clone());
            let _ = s.save();
            let snapshot = s.clone();
            drop(s);
            let _ = weak.upgrade_in_event_loop(move |ui| {
                ui.set_devices(rows(&snapshot));
                ui.set_channels(channel_rows(&snapshot));
            });
            status(weak, label.clone(), true);
            let ir_mode = device.transport == Transport::Infrared;
            let _ = weak.upgrade_in_event_loop(move |ui| ui.global::<Feedback>().set_ir(ir_mode));
            (Some(r), label)
        }
        Err(e) => {
            let _ = weak.upgrade_in_event_loop(|ui| ui.global::<Feedback>().set_ir(false));
            status(weak, e.to_string(), false);
            (None, label)
        }
    }
}

/// Our Android TV certificate: made once (on the first pairing), then kept in the store.
fn android_identity(store: &Arc<Mutex<Store>>) -> omacontrol_core::Result<Identity> {
    if let Some(id) = store.lock().unwrap().android_tv_identity.clone() {
        return Ok(id);
    }
    let id = Identity::generate()?;
    let mut s = store.lock().unwrap();
    s.android_tv_identity = Some(id.clone());
    let _ = s.save();
    Ok(id)
}

fn report(
    weak: &slint::Weak<AppWindow>,
    label: &str,
    result: omacontrol_core::Result<()>,
    status: &impl Fn(&slint::Weak<AppWindow>, String, bool),
) {
    match result {
        Ok(()) => status(weak, label.to_string(), true),
        Err(e) => status(weak, e.to_string(), false),
    }
}

/// Android entry point.
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: slint::android::AndroidApp) {
    android_logger::init_once(android_logger::Config::default().with_max_level(log::LevelFilter::Info));
    let dir = app.internal_data_path().unwrap_or_else(std::env::temp_dir);
    slint::android::init(app).expect("Slint starts on Android");
    // Room for the status bar and the gesture bar until Slint reports the real insets.
    run(dir, 36.0, 16.0);
}
