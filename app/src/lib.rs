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

use omacontrol_core::{connect, discover, App, Brand, Device, IrTransmitter, Key, Remote, Store, Theme as OmaTheme, Transport};
use slint::{Color, ModelRc, SharedString, VecModel};

slint::include_modules!();

#[cfg(target_os = "android")]
mod android_ir;

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
    Connect(Device),
    Press(Key),
    Text(String),
    Launch(App),
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
            meta: if d.transport == Transport::Infrared { format!("{} · IR", d.brand.label()).into() } else if d.model.is_empty() { d.brand.label().into() } else { format!("{} · {}", d.model, d.brand.label()).into() },
            ip: if d.transport == Transport::Infrared { "IR".into() } else { d.ip.rsplit('.').next().unwrap_or_default().into() },
            current: store.current.as_deref() == Some(d.ip.as_str()),
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
            ui.set_tv_name(d.name.clone().into());
            ui.set_page(1);
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
    if let Some(d) = { let s = store.lock().unwrap(); s.current.as_deref().and_then(|ip| s.device(ip)).cloned() } {
        let _ = jobs.send(Job::Connect(d));
    }
    let _ = jobs.send(Job::Scan);

    let tx = jobs.clone();
    ui.on_rescan(move || { let _ = tx.send(Job::Scan); });

    let (tx, st, weak) = (jobs.clone(), store.clone(), ui.as_weak());
    ui.on_pick(move |i| {
        let device = st.lock().unwrap().devices.get(i as usize).cloned();
        if let (Some(d), Some(ui)) = (device, weak.upgrade()) {
            ui.set_tv_name(d.name.clone().into());
            ui.set_page(1);
            let _ = tx.send(Job::Connect(d));
        }
    });

    let (tx, st, weak) = (jobs.clone(), store.clone(), ui.as_weak());
    ui.on_add_ip(move |ip| {
        let ip = ip.trim().to_string();
        if ip.is_empty() {
            return;
        }
        // A TV added by hand: try the brands one by one, Roku first (it needs no pairing).
        let probe = probe_brand(&ip);
        let d = Device::new(&format!("TV {ip}"), "", probe, &ip);
        let mut s = st.lock().unwrap();
        s.upsert(d.clone());
        let _ = s.save();
        if let Some(ui) = weak.upgrade() {
            ui.set_devices(rows(&s));
            ui.set_tv_name(d.name.clone().into());
            ui.set_page(1);
        }
        let _ = tx.send(Job::Connect(d));
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
            ui.set_tv_name(d.name.clone().into());
            ui.set_page(1);
        }
        let _ = tx.send(Job::Connect(d));
    });

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
                    label = if device.transport == Transport::Infrared { format!("{} · IR", device.brand.label()) } else { device.brand.label().to_string() };
                    status(&weak, format!("connecting · {label}"), false);
                    if device.transport == Transport::Network && matches!(device.brand, Brand::Samsung | Brand::Lg) && device.token.is_none() {
                        status(&weak, "say yes on the TV…".into(), false);
                    }
                    match connect(&device, ir.clone()) {
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
                            remote = Some(r);
                            status(&weak, label.clone(), true);
                        }
                        Err(e) => {
                            remote = None;
                            status(&weak, e.to_string(), false);
                        }
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
