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
use omacontrol_core::{
    connect, connect_android_tv, discover, AcChange, AcProtocol, App, Brand, Device, Error,
    IrTransmitter, Key, Remote, Store, Theme as OmaTheme, Transport,
};
use slint::{Color, ModelRc, SharedString, VecModel};

slint::include_modules!();

#[cfg(target_os = "android")]
mod android_ir;
#[cfg(target_os = "android")]
mod android_url;
#[cfg(target_os = "android")]
mod android_voice;
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
        let opener = if cfg!(target_os = "macos") {
            "open"
        } else if cfg!(windows) {
            "explorer"
        } else {
            "xdg-open"
        };
        let _ = std::process::Command::new(opener).arg(url).spawn();
    }
}

/// "0.3.0 · Android", for the feedback email.
fn version_line() -> String {
    let os = if cfg!(target_os = "android") {
        "Android"
    } else {
        std::env::consts::OS
    };
    format!("{} · {os}", env!("CARGO_PKG_VERSION"))
}

enum TvStatus {
    Pairing,
    NotPaired,
    PickTv,
    Looking,
    Connecting(String),
    Approval,
    AskingCode,
    TypeCode,
    Connected(String),
    PairError(String),
    Error(String),
}

fn update_tv_status(weak: &slint::Weak<AppWindow>, status: TvStatus, online: bool) {
    let (code, detail) = match status {
        TvStatus::Pairing => ("pairing", String::new()),
        TvStatus::NotPaired => ("not-paired", String::new()),
        TvStatus::PickTv => ("pick-tv", String::new()),
        TvStatus::Looking => ("looking", String::new()),
        TvStatus::Connecting(detail) => ("connecting", detail),
        TvStatus::Approval => ("approval", String::new()),
        TvStatus::AskingCode => ("asking-code", String::new()),
        TvStatus::TypeCode => ("type-code", String::new()),
        TvStatus::Connected(detail) => ("connected", detail),
        TvStatus::PairError(detail) => ("pair-error", detail),
        TvStatus::Error(detail) => ("error", detail),
    };
    let code = code.to_string();
    let _ = weak.upgrade_in_event_loop(move |ui| {
        ui.set_tv_status_code(code.into());
        ui.set_tv_status_detail(detail.into());
        ui.set_online(online);
    });
}

fn language_for_locale(locale: &str) -> &'static str {
    let locale = locale
        .trim()
        .split(['.', '@'])
        .next()
        .unwrap_or_default()
        .replace('-', "_");
    let normalized = locale.to_ascii_lowercase();
    let language = normalized.split('_').next().unwrap_or_default();
    match language {
        "de" => "de",
        "es" => "es",
        "fr" => "fr",
        "hi" => "hi",
        "it" => "it",
        "ja" => "ja",
        "pt" => "pt",
        "ru" => "ru",
        "tr" => "tr",
        "zh" if !normalized.contains("hant")
            && !["_tw", "_hk", "_mo"]
                .iter()
                .any(|region| normalized.ends_with(region)) =>
        {
            "zh_CN"
        }
        _ => "en",
    }
}

fn valid_language(language: &str) -> bool {
    matches!(
        language,
        "system" | "en" | "de" | "es" | "fr" | "hi" | "it" | "ja" | "pt" | "ru" | "tr" | "zh_CN"
    )
}

fn select_translation(language: &str) {
    let selected = if language == "system" {
        #[cfg(target_os = "android")]
        let locale = match android_language_tag() {
            Ok(locale) => locale,
            Err(error) => {
                log::warn!("could not read Android system language: {error}");
                "en".to_string()
            }
        };
        #[cfg(not(target_os = "android"))]
        let locale = ["LC_ALL", "LC_MESSAGES", "LANG"]
            .iter()
            .filter_map(|name| std::env::var(name).ok())
            .find(|value| !value.is_empty())
            .unwrap_or_else(|| "en".to_string());
        language_for_locale(&locale)
    } else {
        language
    };
    if let Err(error) = slint::select_bundled_translation(selected) {
        log::warn!("could not select translation {selected}: {error}");
        if let Err(fallback_error) = slint::select_bundled_translation("en") {
            log::error!("could not select English translation fallback: {fallback_error}");
        }
    }
}

#[cfg(target_os = "android")]
fn android_language_tag() -> Result<String, jni::errors::Error> {
    use jni::objects::{JObject, JString, JValue};
    use jni::JavaVM;

    let context = ndk_context::android_context();
    let vm = unsafe { JavaVM::from_raw(context.vm().cast()) }?;
    let mut env = vm.attach_current_thread()?;
    let context = unsafe { JObject::from_raw(context.context().cast()) };
    let resources = env
        .call_method(
            &context,
            "getResources",
            "()Landroid/content/res/Resources;",
            &[],
        )?
        .l()?;
    let configuration = env
        .call_method(
            &resources,
            "getConfiguration",
            "()Landroid/content/res/Configuration;",
            &[],
        )?
        .l()?;
    let locales = env
        .call_method(
            &configuration,
            "getLocales",
            "()Landroid/os/LocaleList;",
            &[],
        )?
        .l()?;
    let locale = env
        .call_method(&locales, "get", "(I)Ljava/util/Locale;", &[JValue::Int(0)])?
        .l()?;
    let tag = env
        .call_method(&locale, "toLanguageTag", "()Ljava/lang/String;", &[])?
        .l()?;
    let language_tag: String = env.get_string(&JString::from(tag))?.into();
    Ok(language_tag)
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
    AcScan {
        candidates: Vec<(u8, u8)>,
        cancel: Arc<AtomicBool>,
        dismissed: Arc<AtomicBool>,
    },
    /// A saved channel: its digits, one after another.
    Tune(String),
    /// The code an Android TV shows while pairing.
    Pair(String),
    CancelPair,
    #[cfg(target_os = "android")]
    VoiceStart,
    #[cfg(target_os = "android")]
    VoiceAudio(Vec<u8>),
    #[cfg(target_os = "android")]
    VoiceStop,
    VoiceError(String),
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
    ui.set_ac_status_error(false);
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
    ui.set_ac_setup_error("".into());
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
    std::env::var_os("HOME")
        .map(|home| PathBuf::from(home).join(".local/state/omarchy/current/theme/colors.toml"))
}

#[cfg(target_os = "linux")]
fn load_omarchy_theme() -> Result<(OmaTheme, String), String> {
    let path = omarchy_colors_path().ok_or_else(|| "HOME is not set".to_string())?;
    let source = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let theme =
        OmaTheme::from_omarchy_colors(OMARCHY_THEME_NAME, &source).map_err(|e| e.to_string())?;
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
fn watch_omarchy_theme(
    weak: slint::Weak<AppWindow>,
    store: Arc<Mutex<Store>>,
    initial_source: String,
) -> slint::Timer {
    let timer = slint::Timer::default();
    let mut last_observed = Some(initial_source);
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_secs(1),
        move || {
            let selected = store.lock().unwrap().theme.clone();
            if selected
                .as_deref()
                .is_some_and(|name| name != OMARCHY_THEME_NAME)
            {
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
        },
    );
    timer
}

/// The saved channels of the current TV.
fn channel_rows(store: &Store) -> ModelRc<ChannelRow> {
    let rows: Vec<ChannelRow> = store
        .current
        .as_deref()
        .and_then(|ip| store.device(ip))
        .map(|d| {
            d.channels
                .iter()
                .map(|c| ChannelRow {
                    number: c.number.clone().into(),
                    name: c.name.clone().into(),
                })
                .collect()
        })
        .unwrap_or_default();
    ModelRc::from(Rc::new(VecModel::from(rows)))
}

fn rows(store: &Store, archived: bool) -> ModelRc<DeviceRow> {
    let rows: Vec<DeviceRow> = store
        .devices
        .iter()
        .filter(|d| d.archived == archived)
        .map(|d| DeviceRow {
            id: d.ip.clone().into(),
            name: d.name.clone().into(),
            meta: if d.is_ac() {
                match (&d.ac_protocol, d.ac_code.is_some()) {
                    (_, true) => "code · IR".into(),
                    (Some(p), false) => format!("{} · IR", p.label()).into(),
                    (None, false) => "IR · tap to set up".into(),
                }
            } else if d.transport == Transport::Infrared {
                format!("{} · IR", d.brand.label()).into()
            } else if d.model.is_empty() {
                d.brand.label().into()
            } else {
                format!("{} · {}", d.model, d.brand.label()).into()
            },
            ip: if d.transport == Transport::Infrared {
                "IR".into()
            } else {
                d.ip.rsplit('.').next().unwrap_or_default().into()
            },
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
    let selected_language = store
        .lock()
        .unwrap()
        .language
        .as_deref()
        .unwrap_or("system")
        .to_string();
    select_translation(&selected_language);
    ui.set_language(selected_language.into());

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
        let theme = if s.theme.as_deref() == Some(OMARCHY_THEME_NAME)
            || (s.theme.is_none() && live_theme.is_some())
        {
            live_theme
                .as_ref()
                .map(|(theme, _)| theme.clone())
                .unwrap_or_else(|| OmaTheme::named("Tokyo Night"))
        } else {
            OmaTheme::named(s.theme.as_deref().unwrap_or("Tokyo Night"))
        };
        #[cfg(not(target_os = "linux"))]
        let theme = OmaTheme::named(s.theme.as_deref().unwrap_or("Tokyo Night"));
        apply_theme(&ui, &theme);
        ui.set_voice_supported(false);
        ui.set_devices(rows(&s, ui.get_archived_view()));
        ui.set_channels(channel_rows(&s));
        if let Some(d) = s.current.as_deref().and_then(|ip| s.device(ip)) {
            if d.is_ac() {
                show_ac(&ui, d);
                ui.set_page(if d.ac_protocol.is_some() { 4 } else { 0 });
            } else {
                ui.set_tv_name(d.name.clone().into());
                ui.set_page(1);
                ui.set_voice_supported(false);
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
    let languages: Vec<SharedString> = [
        "system", "en", "de", "es", "fr", "hi", "it", "ja", "pt", "ru", "tr", "zh_CN",
    ]
    .into_iter()
    .map(Into::into)
    .collect();
    ui.set_languages(ModelRc::from(Rc::new(VecModel::from(languages))));

    let ir = blaster();
    ui.set_has_ir(ir.is_some());
    let jobs = spawn_worker(ui.as_weak(), store.clone(), ir);

    let voice_capture = Rc::new(RefCell::new(None::<Arc<AtomicBool>>));
    let (tx, weak, capture) = (jobs.clone(), ui.as_weak(), voice_capture.clone());
    ui.on_voice_start(move || {
        let Some(ui) = weak.upgrade() else { return };
        if !ui.get_voice_supported() || capture.borrow().is_some() {
            return;
        }
        #[cfg(target_os = "android")]
        match android_voice::start(tx.clone(), weak.clone()) {
            Ok(stop) => {
                *capture.borrow_mut() = Some(stop);
                ui.set_voice_active(true);
                ui.set_voice_status_code("starting".into());
                ui.set_voice_status_detail("".into());
            }
            Err(android_voice::StartError::Permission) => {
                ui.set_voice_active(false);
                ui.set_voice_status_code("permission".into());
                ui.set_voice_status_detail("".into());
            }
            Err(android_voice::StartError::Other(error)) => {
                ui.set_voice_active(false);
                ui.set_voice_status_code("error".into());
                ui.set_voice_status_detail(error.into());
            }
        }
        #[cfg(not(target_os = "android"))]
        {
            let _ = tx.send(Job::VoiceError(
                "microphone voice search is available on Android only".into(),
            ));
        }
    });
    let (weak, capture) = (ui.as_weak(), voice_capture);
    ui.on_voice_stop(move || {
        if let Some(stop) = capture.borrow_mut().take() {
            stop.store(true, Ordering::Relaxed);
        }
        if let Some(ui) = weak.upgrade() {
            ui.set_voice_active(false);
        }
    });

    // Reconnect to the last TV, and look for others.
    if let Some(d) = {
        let s = store.lock().unwrap();
        s.current
            .as_deref()
            .and_then(|ip| s.device(ip))
            .filter(|d| !d.is_ac())
            .cloned()
    } {
        let _ = jobs.send(Job::Connect(d));
    }
    let _ = jobs.send(Job::Scan);

    let tx = jobs.clone();
    ui.on_rescan(move || {
        let _ = tx.send(Job::Scan);
    });

    let (tx, st, weak) = (jobs.clone(), store.clone(), ui.as_weak());
    ui.on_pick(move |id| {
        let id = id.to_string();
        let device = st
            .lock()
            .unwrap()
            .device(&id)
            .filter(|d| !d.archived)
            .cloned();
        if let (Some(d), Some(ui)) = (device.clone(), weak.upgrade()) {
            if d.is_ac() {
                let mut s = st.lock().unwrap();
                s.current = Some(d.ip.clone());
                if let Err(error) = s.save() {
                    log::warn!("could not save current device selection: {error}");
                }
                ui.set_devices(rows(&s, ui.get_archived_view()));
                ui.set_voice_supported(false);
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
            ui.set_voice_supported(false);
            ui.set_pairing(false);
            ui.set_tv_name(d.name.clone().into());
            ui.set_page(1);
            {
                // Show this TV's saved channels right away (the connection catches up).
                let mut s = st.lock().unwrap();
                s.current = Some(d.ip.clone());
                ui.set_channels(channel_rows(&s));
                ui.set_devices(rows(&s, ui.get_archived_view()));
                if let Err(error) = s.save() {
                    log::warn!("could not save current device selection: {error}");
                }
            }
            let _ = tx.send(Job::Connect(d));
        }
    });

    let (st, weak) = (store.clone(), ui.as_weak());
    ui.on_set_archived_view(move |archived| {
        if let Some(ui) = weak.upgrade() {
            ui.set_archived_view(archived);
            let s = st.lock().unwrap();
            ui.set_devices(rows(&s, archived));
        }
    });

    let (st, weak) = (store.clone(), ui.as_weak());
    ui.on_archive_device(move |id, archived| {
        let id = id.to_string();
        let mut s = st.lock().unwrap();
        let was_current = s.current.as_deref() == Some(id.as_str());
        if !s.archive_device(&id, archived) {
            return;
        }
        if let Err(error) = s.save() {
            log::warn!("could not save device archive change: {error}");
        }
        if let Some(ui) = weak.upgrade() {
            ui.set_devices(rows(&s, ui.get_archived_view()));
            if was_current && archived {
                ui.set_page(0);
                ui.set_current_is_ac(false);
                ui.set_voice_supported(false);
                ui.set_channels(channel_rows(&s));
            }
        }
    });

    let (st, weak) = (store.clone(), ui.as_weak());
    ui.on_remove_device(move |id| {
        let id = id.to_string();
        let mut s = st.lock().unwrap();
        let was_current = s.current.as_deref() == Some(id.as_str());
        if !s.remove_device(&id) {
            return;
        }
        if let Err(error) = s.save() {
            log::warn!("could not save device removal: {error}");
        }
        if let Some(ui) = weak.upgrade() {
            ui.set_devices(rows(&s, ui.get_archived_view()));
            if was_current {
                ui.set_page(0);
                ui.set_current_is_ac(false);
                ui.set_voice_supported(false);
                ui.set_channels(channel_rows(&s));
            }
        }
    });

    let (st, weak) = (store.clone(), ui.as_weak());
    ui.on_rename_device(move |id, name| {
        let id = id.to_string();
        let name = name.to_string();
        let mut s = st.lock().unwrap();
        if !s.rename_device(&id, &name) {
            return;
        }
        if let Err(error) = s.save() {
            log::warn!("could not save device rename: {error}");
        }
        if let Some(ui) = weak.upgrade() {
            ui.set_devices(rows(&s, ui.get_archived_view()));
            if s.current.as_deref() == Some(id.as_str()) {
                if let Some(device) = s.device(&id) {
                    if device.is_ac() {
                        show_ac(&ui, device);
                    } else {
                        ui.set_tv_name(device.name.clone().into());
                    }
                }
            }
        }
    });

    let (st, weak) = (store.clone(), ui.as_weak());
    ui.on_move_device(move |id, steps| {
        let id = id.to_string();
        let mut s = st.lock().unwrap();
        if !s.move_device(&id, steps as isize) {
            return;
        }
        if let Err(error) = s.save() {
            log::warn!("could not save device order: {error}");
        }
        if let Some(ui) = weak.upgrade() {
            ui.set_devices(rows(&s, ui.get_archived_view()));
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
            ui.set_devices(rows(&s, ui.get_archived_view()));
            ui.set_current_is_ac(false);
            ui.set_voice_supported(false);
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
            ui.set_devices(rows(&s, ui.get_archived_view()));
            ui.set_voice_supported(false);
            show_ac(&ui, &d);
            idx.set(0);
            show_ac_setup(&ui, 0, false);
            ui.set_page(5);
        }
    });

    let pasted = Rc::new(RefCell::new(String::new()));
    let typed = pasted.clone();
    ui.on_ac_code_changed(move |text| {
        *typed.borrow_mut() = text.to_string();
    });

    let scan_cancel = Rc::new(RefCell::new(None::<Arc<AtomicBool>>));
    let scan_dismissed = Rc::new(RefCell::new(None::<Arc<AtomicBool>>));
    let scan_candidates = Rc::new(RefCell::new(Vec::<(u8, u8)>::new()));
    let scan_index = Rc::new(Cell::new(0usize));
    let (tx, st, weak, idx, pasted, scan_cancel_state, scan_dismissed_state, scan_candidates_state, selected_scan_index) = (
        jobs.clone(),
        store.clone(),
        ui.as_weak(),
        setup_index.clone(),
        pasted,
        scan_cancel.clone(),
        scan_dismissed.clone(),
        scan_candidates.clone(),
        scan_index.clone(),
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
            "code" => {
                ui.set_ac_setup_error("".into());
                match omacontrol_core::ac::custom_frames(&pasted.borrow().clone()) {
                    Ok(frames) => {
                        let _ = tx.send(Job::Ac(frames));
                        show_ac_setup(&ui, idx.get(), true);
                    }
                    Err(e) => ui.set_ac_setup_error(e.to_string().into()),
                }
            }
            "scan-confirm" => {
                let cancel = Arc::new(AtomicBool::new(false));
                let dismissed = Arc::new(AtomicBool::new(false));
                *scan_cancel_state.borrow_mut() = Some(cancel.clone());
                *scan_dismissed_state.borrow_mut() = Some(dismissed.clone());
                ui.set_ac_scan_active(true);
                ui.set_ac_scan_finished(false);
                ui.set_ac_scan_cancelled(false);
                ui.set_ac_scan_count(0);
                let candidates = omacontrol_core::ac::nec_autotest_candidates();
                *scan_candidates_state.borrow_mut() = candidates.clone();
                selected_scan_index.set(0);
                ui.set_ac_scan_index(0);
                ui.set_ac_scan_review(false);
                ui.set_ac_scan_total(candidates.len() as i32);
                ui.set_ac_scan_code("".into());
                ui.set_ac_scan_result("".into());
                if let Err(e) = tx.send(Job::AcScan {
                    candidates,
                    cancel,
                    dismissed,
                }) {
                    *scan_cancel_state.borrow_mut() = None;
                    ui.set_ac_scan_active(false);
                    ui.set_ac_scan_finished(true);
                    ui.set_ac_scan_cancelled(false);
                    ui.set_ac_scan_result(e.to_string().into());
                }
            }
            "scan-found" => {
                let code = ui.get_ac_scan_code().to_string();
                if code.is_empty() {
                    return;
                }
                let selected = ui.get_ac_scan_count().saturating_sub(1) as usize;
                selected_scan_index.set(selected);
                ui.set_ac_scan_index(selected as i32);
                ui.set_ac_scan_review(true);
                ui.set_ac_scan_active(false);
                if let Some(cancel) = scan_cancel_state.borrow_mut().take() {
                    cancel.store(true, Ordering::Relaxed);
                }
            }
            "scan-save" => {
                if let Some(dismissed) = scan_dismissed_state.borrow().as_ref() {
                    dismissed.store(true, Ordering::Relaxed);
                }
                let code = ui.get_ac_scan_code().to_string();
                if code.is_empty() {
                    return;
                }
                let mut s = st.lock().unwrap();
                let Some(ip) = s.current.clone() else { return };
                if let Some(d) = s.devices.iter_mut().find(|d| d.ip == ip && d.is_ac()) {
                    d.ac_protocol = None;
                    d.ac_code = Some(code);
                    d.ac_state = omacontrol_core::ac::TEST_STATE;
                    let d = d.clone();
                    let _ = s.save();
                    ui.set_devices(rows(&s, ui.get_archived_view()));
                    show_ac(&ui, &d);
                    ui.set_ac_scan_active(false);
                    ui.set_ac_scan_finished(false);
                    ui.set_ac_scan_cancelled(false);
                    ui.set_ac_scan_review(false);
                    ui.set_page(4);
                }
            }
            "scan-dismiss" => {
                if let Some(dismissed) = scan_dismissed_state.borrow().as_ref() {
                    dismissed.store(true, Ordering::Relaxed);
                }
                *scan_cancel_state.borrow_mut() = None;
                ui.set_ac_scan_finished(false);
                ui.set_ac_scan_review(false);
                ui.set_ac_scan_result("".into());
            }
            _ => {
                let mut s = st.lock().unwrap();
                let Some(ip) = s.current.clone() else { return };
                let code = pasted.borrow().clone();
                let pasted_code = (!code.is_empty()
                    && omacontrol_core::ac::custom_frames(&code).is_ok())
                .then_some(code);
                if let Some(d) = s.devices.iter_mut().find(|d| d.ip == ip) {
                    d.ac_protocol = if pasted_code.is_some() {
                        None
                    } else {
                        Some(protocol)
                    };
                    d.ac_code = pasted_code;
                    d.ac_state = omacontrol_core::ac::TEST_STATE;
                    let d = d.clone();
                    let _ = s.save();
                    ui.set_devices(rows(&s, ui.get_archived_view()));
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

    let (tx, candidates, selected, weak) =
        (jobs.clone(), scan_candidates, scan_index, ui.as_weak());
    ui.on_ac_scan_navigate(move |delta| {
        let all = candidates.borrow();
        if all.is_empty() {
            return;
        }
        let next =
            (selected.get() as isize + delta as isize).clamp(0, all.len() as isize - 1) as usize;
        selected.set(next);
        let (address, command) = all[next];
        let code = format!(
            "NEC {:08X}",
            omacontrol_core::ir::nec_code(address, command)
        );
        if let Some(ui) = weak.upgrade() {
            ui.set_ac_scan_index(next as i32);
            ui.set_ac_scan_code(code.into());
            ui.set_ac_scan_result("".into());
        }
        if let Err(error) = tx.send(Job::Ac(vec![omacontrol_core::ir::nec(address, command)])) {
            if let Some(ui) = weak.upgrade() {
                ui.set_ac_scan_result(error.to_string().into());
            }
        }
    });

    let (tx, st, weak) = (jobs.clone(), store.clone(), ui.as_weak());
    ui.on_ac_action(move |action| {
        let Some(ui) = weak.upgrade() else { return };
        let mut s = st.lock().unwrap();
        let Some(ip) = s.current.clone() else { return };
        let Some(d) = s.devices.iter_mut().find(|d| d.ip == ip && d.is_ac()) else {
            return;
        };
        if let Some(code) = d.ac_code.clone() {
            // A pasted code is one command (usually power); the state keys have nothing to send.
            if action.as_str() != "power" {
                return;
            }
            d.ac_state.power = !d.ac_state.power;
            let Ok(frames) = omacontrol_core::ac::custom_frames(&code) else {
                return;
            };
            let d = d.clone();
            let _ = s.save();
            drop(s);
            show_ac(&ui, &d);
            let _ = tx.send(Job::Ac(frames));
            return;
        }
        let Some(protocol) = d.ac_protocol else {
            return;
        };
        let state = &mut d.ac_state;
        let change = match action.as_str() {
            "power" => {
                state.power = !state.power;
                AcChange::State
            }
            "mode" => {
                state.mode = state.mode.next();
                AcChange::State
            }
            "fan" => {
                state.fan = state.fan.next();
                AcChange::State
            }
            "warmer" => {
                state.warmer();
                AcChange::State
            }
            "cooler" => {
                state.cooler();
                AcChange::State
            }
            "swing" => {
                state.swing = !state.swing;
                AcChange::Swing
            }
            "sleep" => {
                state.sleep = !state.sleep;
                AcChange::Sleep
            }
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
    ui.on_send_text(move |t| {
        let _ = tx.send(Job::Text(t.to_string()));
    });

    // Saved channels.
    let tx = jobs.clone();
    ui.on_tune(move |number| {
        let _ = tx.send(Job::Tune(number.to_string()));
    });
    let (st, weak) = (store.clone(), ui.as_weak());
    ui.on_save_channel(move |number, name| {
        let mut s = st.lock().unwrap();
        let Some(ip) = s.current.clone() else { return };
        let changed = s
            .devices
            .iter_mut()
            .find(|d| d.ip == ip)
            .is_some_and(|d| d.save_channel(&number, &name));
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
        if s.devices
            .iter_mut()
            .find(|d| d.ip == ip)
            .is_some_and(|d| d.remove_channel(&number))
        {
            let _ = s.save();
            if let Some(ui) = weak.upgrade() {
                ui.set_channels(channel_rows(&s));
            }
        }
    });

    // Android TV pairing.
    let tx = jobs.clone();
    ui.on_pair(move |code| {
        let _ = tx.send(Job::Pair(code.to_string()));
    });
    let tx = jobs.clone();
    ui.on_cancel_pair(move || {
        let _ = tx.send(Job::CancelPair);
    });

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
            let code = match why {
                "that email doesn't look right" => "email-invalid",
                "add a title" => "title-required",
                "write a few more words" => "message-short",
                _ => "invalid",
            };
            ui.set_fb_status_code(code.into());
            ui.set_fb_status_detail("".into());
            return;
        }
        ui.set_fb_sending(true);
        ui.set_fb_status_code("sending".into());
        ui.set_fb_status_detail("".into());
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

    let (st, weak) = (store.clone(), ui.as_weak());
    ui.on_pick_language(move |name| {
        if !valid_language(&name) {
            log::warn!("ignoring unsupported language selection: {name}");
            return;
        }
        select_translation(&name);
        let mut s = st.lock().unwrap();
        s.language = (name != "system").then(|| name.to_string());
        if let Err(error) = s.save() {
            log::warn!("could not save language preference: {error}");
        }
        if let Some(ui) = weak.upgrade() {
            ui.set_language(name);
        }
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
fn spawn_worker(
    weak: slint::Weak<AppWindow>,
    store: Arc<Mutex<Store>>,
    ir: Option<Arc<dyn IrTransmitter>>,
) -> Sender<Job> {
    let (tx, rx) = mpsc::channel::<Job>();
    let status = |weak: &slint::Weak<AppWindow>, state: TvStatus, online: bool| {
        update_tv_status(weak, state, online)
    };
    thread::spawn(move || {
        let mut remote: Option<Box<dyn Remote>> = None;
        let mut label = String::new();
        #[cfg(target_os = "android")]
        let mut voice_ready = false;
        #[cfg(target_os = "android")]
        let mut voice_error = false;
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
                        ui.set_devices(rows(&snapshot, ui.get_archived_view()));
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
                    let Some((device, pairing)) = pending.take() else {
                        continue;
                    };
                    // A typo is caught before anything is sent, and the TV keeps its code up.
                    if let Err(e) = pairing.check(&code) {
                        status(&weak, TvStatus::Error(e.to_string()), false);
                        pending = Some((device, pairing));
                        continue;
                    }
                    status(&weak, TvStatus::Pairing, false);
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
                            let (r, l) =
                                connect_device(&weak, &store, &ir, &paired, &status, &mut pending);
                            remote = r;
                            label = l;
                        }
                        Err(e) => {
                            let _ = weak.upgrade_in_event_loop(|ui| ui.set_pairing(false));
                            status(&weak, TvStatus::PairError(e.to_string()), false);
                        }
                    }
                }
                Job::CancelPair => {
                    pending = None;
                    let _ = weak.upgrade_in_event_loop(|ui| ui.set_pairing(false));
                    status(&weak, TvStatus::NotPaired, false);
                }
                #[cfg(target_os = "android")]
                Job::VoiceStart => {
                    voice_error = false;
                    let result = remote
                        .as_mut()
                        .ok_or_else(|| "pick a TV first".to_string())
                        .and_then(|remote| remote.voice_start().map_err(|error| error.to_string()));
                    match result {
                        Ok(()) => {
                            voice_ready = true;
                            let _ = weak.upgrade_in_event_loop(|ui| {
                                ui.set_voice_active(true);
                                ui.set_voice_status_code("active".into());
                                ui.set_voice_status_detail("".into());
                            });
                        }
                        Err(error) => {
                            voice_error = true;
                            let _ = weak.upgrade_in_event_loop(move |ui| {
                                ui.set_voice_active(false);
                                ui.set_voice_status_code("error".into());
                                ui.set_voice_status_detail(error.into());
                            });
                        }
                    }
                }
                #[cfg(target_os = "android")]
                Job::VoiceAudio(samples) => {
                    if voice_ready {
                        if let Some(remote) = remote.as_mut() {
                            if let Err(error) = remote.voice_audio(&samples) {
                                voice_ready = false;
                                voice_error = true;
                                let detail = error.to_string();
                                let _ = weak.upgrade_in_event_loop(move |ui| {
                                    ui.set_voice_active(false);
                                    ui.set_voice_status_code("error".into());
                                    ui.set_voice_status_detail(detail.into());
                                });
                            }
                        }
                    }
                }
                #[cfg(target_os = "android")]
                Job::VoiceStop => {
                    if voice_ready {
                        if let Some(remote) = remote.as_mut() {
                            if let Err(error) = remote.voice_stop() {
                                voice_error = true;
                                let detail = error.to_string();
                                let _ = weak.upgrade_in_event_loop(move |ui| {
                                    ui.set_voice_status_code("error".into());
                                    ui.set_voice_status_detail(detail.into());
                                });
                            }
                        }
                    }
                    voice_ready = false;
                    let keep_error = voice_error;
                    let _ = weak.upgrade_in_event_loop(move |ui| {
                        ui.set_voice_active(false);
                        if !keep_error {
                            ui.set_voice_status_code("".into());
                            ui.set_voice_status_detail("".into());
                        }
                    });
                }
                Job::VoiceError(error) => {
                    #[cfg(target_os = "android")]
                    {
                        voice_error = true;
                    }
                    let _ = weak.upgrade_in_event_loop(move |ui| {
                        ui.set_voice_active(false);
                        ui.set_voice_status_code("error".into());
                        ui.set_voice_status_detail(error.into());
                    });
                }
                Job::Tune(number) => {
                    let Some(r) = remote.as_mut() else {
                        status(&weak, TvStatus::PickTv, false);
                        continue;
                    };
                    let mut result = Ok(());
                    for c in number.chars() {
                        let key = if c == '-' {
                            Key::Dash
                        } else if let Some(d) = c.to_digit(10) {
                            Key::Digit(d as u8)
                        } else {
                            continue;
                        };
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
                                ui.set_fb_status_code("sent".into());
                                ui.set_fb_status_detail("".into());
                                ui.set_fb_title("".into());
                                ui.set_fb_message("".into());
                            }
                            Err(e) => {
                                ui.set_fb_status_code("failed".into());
                                ui.set_fb_status_detail(e.to_string().into());
                            }
                        }
                    });
                }
                Job::Probe(ip) => {
                    // A TV added by hand: try the brands one by one, Roku first (it needs no pairing).
                    let name = format!("TV {ip}");
                    let shown = name.clone();
                    status(&weak, TvStatus::Looking, false);
                    let _ = weak.upgrade_in_event_loop(move |ui| {
                        ui.set_current_is_ac(false);
                        ui.set_tv_name(shown.into());
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
                        let _ = weak.upgrade_in_event_loop(move |ui| {
                            ui.set_devices(rows(&snapshot, ui.get_archived_view()))
                        });
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
                        let _ = weak.upgrade_in_event_loop(move |ui| {
                            ui.set_ac_status(e.into());
                            ui.set_ac_status_error(true);
                        });
                    }
                }
                Job::AcScan {
                    candidates,
                    cancel,
                    dismissed,
                } => {
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
                                let code = format!(
                                    "NEC {:08X}",
                                    omacontrol_core::ir::nec_code(address, command)
                                );
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
                    let completion_dismissed = dismissed.clone();
                    let _ = weak.upgrade_in_event_loop(move |ui| {
                        if completion_dismissed.load(Ordering::Relaxed) {
                            return;
                        }
                        ui.set_ac_scan_active(false);
                        ui.set_ac_scan_finished(true);
                        ui.set_ac_scan_cancelled(stopped);
                        ui.set_ac_scan_result(result.into());
                    });
                }
                Job::Press(_) | Job::Text(_) | Job::Launch(_) if remote.is_none() => {
                    status(&weak, TvStatus::PickTv, false);
                }
                Job::Press(k) => report(&weak, &label, remote.as_mut().unwrap().press(k), &status),
                Job::Text(t) => report(
                    &weak,
                    &label,
                    remote.as_mut().unwrap().type_text(&t),
                    &status,
                ),
                Job::Launch(a) => {
                    report(&weak, &label, remote.as_mut().unwrap().launch(a), &status)
                }
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
    status: &impl Fn(&slint::Weak<AppWindow>, TvStatus, bool),
    pending: &mut Option<(Device, Pairing)>,
) -> (Option<Box<dyn Remote>>, String) {
    let label = if device.transport == Transport::Infrared {
        format!("{} · IR", device.brand.label())
    } else {
        device.brand.label().to_string()
    };
    status(weak, TvStatus::Connecting(label.clone()), false);
    if device.transport == Transport::Network
        && matches!(device.brand, Brand::Samsung | Brand::Lg)
        && device.token.is_none()
    {
        status(weak, TvStatus::Approval, false);
    }
    let android_tv = device.transport == Transport::Network && device.brand == Brand::AndroidTv;
    let connected = if android_tv {
        match android_identity(store) {
            Ok(id) => match connect_android_tv(&device.ip, &id) {
                Err(Error::NotPaired) => {
                    // The TV doesn't know our certificate: ask it for a code.
                    status(weak, TvStatus::AskingCode, false);
                    match Pairing::start(&device.ip, &id, "OmaControl") {
                        Ok(p) => {
                            *pending = Some((device.clone(), p));
                            let _ = weak.upgrade_in_event_loop(|ui| ui.set_pairing(true));
                            status(weak, TvStatus::TypeCode, false);
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
            let voice_supported = cfg!(target_os = "android") && r.supports_voice();
            let mut s = store.lock().unwrap();
            let mut d = device.clone();
            d.token = r.token().or(d.token);
            s.upsert(d);
            s.current = Some(device.ip.clone());
            let _ = s.save();
            let snapshot = s.clone();
            drop(s);
            let _ = weak.upgrade_in_event_loop(move |ui| {
                ui.set_devices(rows(&snapshot, ui.get_archived_view()));
                ui.set_channels(channel_rows(&snapshot));
                ui.set_voice_supported(voice_supported);
            });
            status(weak, TvStatus::Connected(label.clone()), true);
            let ir_mode = device.transport == Transport::Infrared;
            let _ = weak.upgrade_in_event_loop(move |ui| ui.global::<Feedback>().set_ir(ir_mode));
            (Some(r), label)
        }
        Err(e) => {
            let _ = weak.upgrade_in_event_loop(|ui| ui.global::<Feedback>().set_ir(false));
            status(weak, TvStatus::Error(e.to_string()), false);
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
    status: &impl Fn(&slint::Weak<AppWindow>, TvStatus, bool),
) {
    match result {
        Ok(()) => status(weak, TvStatus::Connected(label.to_string()), true),
        Err(e) => status(weak, TvStatus::Error(e.to_string()), false),
    }
}

/// Android entry point.
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: slint::android::AndroidApp) {
    android_logger::init_once(
        android_logger::Config::default().with_max_level(log::LevelFilter::Info),
    );
    let dir = app.internal_data_path().unwrap_or_else(std::env::temp_dir);
    slint::android::init(app).expect("Slint starts on Android");
    // Room for the status bar and the gesture bar until Slint reports the real insets.
    run(dir, 36.0, 16.0);
}

#[cfg(test)]
mod localization_tests {
    use super::language_for_locale;
    use std::collections::{HashMap, HashSet};

    #[test]
    fn selects_supported_system_languages_and_falls_back_to_english() {
        for (locale, expected) in [
            ("de-DE", "de"),
            ("es_MX.UTF-8", "es"),
            ("fr_CA", "fr"),
            ("hi-IN", "hi"),
            ("it-IT", "it"),
            ("ja-JP", "ja"),
            ("pt-BR", "pt"),
            ("ru-RU", "ru"),
            ("tr-TR", "tr"),
            ("zh-Hans-CN", "zh_CN"),
            ("zh-CN.UTF-8", "zh_CN"),
            ("zh-Hant-TW", "en"),
            ("en-GB", "en"),
            ("ar-SA", "en"),
        ] {
            assert_eq!(language_for_locale(locale), expected, "{locale}");
        }
    }

    fn quoted_content(source: &str, quote_at: usize) -> Option<(&str, usize)> {
        let content_start = quote_at + 1;
        let mut escaped = false;
        for (offset, character) in source[content_start..].char_indices() {
            if character == '"' && !escaped {
                let end = content_start + offset;
                return Some((&source[content_start..end], end + 1));
            }
            escaped = character == '\\' && !escaped;
        }
        None
    }

    fn slint_messages(source: &str) -> HashSet<String> {
        let mut messages = HashSet::new();
        let mut cursor = 0;
        while let Some(start) = source[cursor..].find("@tr(\"") {
            let quote_at = cursor + start + 4;
            let (message, end) =
                quoted_content(source, quote_at).expect("translation string closes");
            messages.insert(message.to_string());
            cursor = end;
        }
        messages
    }

    fn po_messages(source: &str) -> HashMap<String, String> {
        let mut messages = HashMap::new();
        let mut msgid = None;
        for line in source.lines() {
            if let Some(value) = line.strip_prefix("msgid ") {
                let (message, _) = quoted_content(value, 0).expect("PO msgid is quoted");
                msgid = Some(message.to_string());
            } else if let Some(value) = line.strip_prefix("msgstr ") {
                let (translation, _) = quoted_content(value, 0).expect("PO msgstr is quoted");
                if let Some(message) = msgid.take() {
                    messages.insert(message, translation.to_string());
                }
            }
        }
        messages
    }

    #[test]
    fn every_screen_message_is_translated_in_each_bundled_locale() {
        let ui_sources = [
            include_str!("../ui/ac.slint"),
            include_str!("../ui/app.slint"),
            include_str!("../ui/devices.slint"),
            include_str!("../ui/keypad.slint"),
            include_str!("../ui/menu.slint"),
            include_str!("../ui/remote.slint"),
            include_str!("../ui/scanner.slint"),
            include_str!("../ui/settings.slint"),
            include_str!("../ui/widgets.slint"),
        ];
        let messages: HashSet<_> = ui_sources
            .iter()
            .flat_map(|source| slint_messages(source))
            .collect();
        let catalogs = [
            (
                "de",
                include_str!("../translations/de/LC_MESSAGES/omacontrol.po"),
            ),
            (
                "es",
                include_str!("../translations/es/LC_MESSAGES/omacontrol.po"),
            ),
            (
                "fr",
                include_str!("../translations/fr/LC_MESSAGES/omacontrol.po"),
            ),
            (
                "hi",
                include_str!("../translations/hi/LC_MESSAGES/omacontrol.po"),
            ),
            (
                "it",
                include_str!("../translations/it/LC_MESSAGES/omacontrol.po"),
            ),
            (
                "ja",
                include_str!("../translations/ja/LC_MESSAGES/omacontrol.po"),
            ),
            (
                "pt",
                include_str!("../translations/pt/LC_MESSAGES/omacontrol.po"),
            ),
            (
                "ru",
                include_str!("../translations/ru/LC_MESSAGES/omacontrol.po"),
            ),
            (
                "tr",
                include_str!("../translations/tr/LC_MESSAGES/omacontrol.po"),
            ),
            (
                "zh_CN",
                include_str!("../translations/zh_CN/LC_MESSAGES/omacontrol.po"),
            ),
        ];
        for (locale, source) in catalogs {
            let catalog = po_messages(source);
            for message in &messages {
                let translation = catalog
                    .get(message)
                    .unwrap_or_else(|| panic!("{locale} is missing {message:?}"));
                assert!(
                    !translation.is_empty(),
                    "{locale} has an empty translation for {message:?}"
                );
                assert_eq!(
                    message.matches("{}").count(),
                    translation.matches("{}").count(),
                    "{locale} has mismatched placeholders for {message:?}"
                );
            }
        }
    }
}
