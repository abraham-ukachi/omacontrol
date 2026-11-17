// Desktop entry point for Omarchy/Linux and development.
fn main() {
    // `--version` has to answer without a window: it is how you check which build is installed.
    if std::env::args().skip(1).any(|a| a == "--version" || a == "-V") {
        println!("omacontrol {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    let dir = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config")))
        .unwrap_or_else(std::env::temp_dir)
        .join("omacontrol");
    omacontrol::run(dir, 0.0, 0.0);
}
