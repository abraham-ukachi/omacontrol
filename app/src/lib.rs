//! # OmaControl
//!
//! The app: the Slint UI (`ui/*.slint`) wired to `omacontrol-core`.
//! Everything that talks to a TV runs on a worker thread, so the UI never waits on the network.

use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use std::cell::Cell;

use omacontrol_core::ir::Pattern;
use omacontrol_core::{connect, discover, AcChange, AcFan, AcMode, AcProtocol, AcState, App, Brand, Device, IrTransmitter, Key, Remote, Store, Theme as OmaTheme, Transport};
use slint::{Color, ModelRc, SharedString, VecModel};

slint::include_modules!();

#[cfg(target_os = "android")]
mod android_ir;
#[cfg(target_os = "android")]
mod haptics;

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
}

/// The state setup sends to see whether a protocol works: on, cool, 22 °C, low fan.
const AC_TEST: AcState = AcState { power: true, mode: AcMode::Cool, fan: AcFan::Low, temp: 22, swing: false, sleep: false };

/// Shows an air conditioner on the AC page.
fn show_ac(ui: &AppWindow, d: &Device) {
    let s = d.ac_state;
    ui.set_current_is_ac(true);
    ui.set_ac_name(d.name.clone().into());
    ui.set_ac_status(match d.ac_protocol {
        Some(p) => format!("{} · IR", p.label()).into(),
        None => "not set up yet".into(),
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

fn rows(store: &Store) -> ModelRc<DeviceRow> {
    let rows: Vec<DeviceRow> = store
        .devices
        .iter()
        .map(|d| DeviceRow {
            name: d.name.clone().into(),
            meta: if d.is_ac() { match d.ac_protocol { Some(p) => format!("{} · IR", p.label()).into(), None => "IR · tap to set up".into() } } else if d.transport == Transport::Infrared { format!("{} · IR", d.brand.label()).into() } else if d.model.is_empty() { d.brand.label().into() } else { format!("{} · {}", d.model, d.brand.label()).into() },
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

    {
        let s = store.lock().unwrap();
        apply_theme(&ui, &OmaTheme::named(s.theme.as_deref().unwrap_or("Tokyo Night")));
        ui.set_devices(rows(&s));
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
            ui.set_tv_name(d.name.clone().into());
            ui.set_page(1);
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

    let (tx, st, weak, idx) = (jobs.clone(), store.clone(), ui.as_weak(), setup_index.clone());
    ui.on_ac_setup(move |step| {
        let Some(ui) = weak.upgrade() else { return };
        let protocol = AcProtocol::ALL[idx.get()];
        match step.as_str() {
            "test" => {
                let _ = tx.send(Job::Ac(protocol.frames(&AC_TEST, AcChange::State)));
                show_ac_setup(&ui, idx.get(), true);
            }
            "next" => {
                idx.set((idx.get() + 1) % AcProtocol::ALL.len());
                show_ac_setup(&ui, idx.get(), false);
            }
            _ => {
                let mut s = st.lock().unwrap();
                let Some(ip) = s.current.clone() else { return };
                if let Some(d) = s.devices.iter_mut().find(|d| d.ip == ip) {
                    d.ac_protocol = Some(protocol);
                    d.ac_state = AC_TEST;
                    let d = d.clone();
                    let _ = s.save();
                    ui.set_devices(rows(&s));
                    show_ac(&ui, &d);
                    ui.set_page(4);
                }
            }
        }
    });

    let (tx, st, weak) = (jobs.clone(), store.clone(), ui.as_weak());
    ui.on_ac_action(move |action| {
        let Some(ui) = weak.upgrade() else { return };
        let mut s = st.lock().unwrap();
        let Some(ip) = s.current.clone() else { return };
        let Some(d) = s.devices.iter_mut().find(|d| d.ip == ip && d.is_ac()) else { return };
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

    let (st, weak) = (store.clone(), ui.as_weak());
    ui.on_pick_theme(move |name| {
        if let Some(ui) = weak.upgrade() {
            apply_theme(&ui, &OmaTheme::named(&name));
        }
        let mut s = st.lock().unwrap();
        s.theme = Some(name.to_string());
        let _ = s.save();
    });

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
                    let (r, l) = connect_device(&weak, &store, &ir, &device, &status);
                    remote = r;
                    label = l;
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
                    let (r, l) = connect_device(&weak, &store, &ir, &device, &status);
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
) -> (Option<Box<dyn Remote>>, String) {
    let label = if device.transport == Transport::Infrared { format!("{} · IR", device.brand.label()) } else { device.brand.label().to_string() };
    status(weak, format!("connecting · {label}"), false);
    if device.transport == Transport::Network && matches!(device.brand, Brand::Samsung | Brand::Lg) && device.token.is_none() {
        status(weak, "say yes on the TV…".into(), false);
    }
    match connect(device, ir.clone()) {
        Ok(r) => {
            let mut s = store.lock().unwrap();
            let mut d = device.clone();
            d.token = r.token().or(d.token);
            s.upsert(d);
            s.current = Some(device.ip.clone());
            let _ = s.save();
            let snapshot = s.clone();
            drop(s);
            let _ = weak.upgrade_in_event_loop(move |ui| ui.set_devices(rows(&snapshot)));
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
