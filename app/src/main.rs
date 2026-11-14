// Desktop entry point (handy while designing; the real target is Android).
fn main() {
    let dir = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config")))
        .unwrap_or_else(std::env::temp_dir)
        .join("omacontrol");
    omacontrol::run(dir, 0.0, 0.0);
}
