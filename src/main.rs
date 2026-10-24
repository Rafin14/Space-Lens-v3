#![windows_subsystem = "windows"]
fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let strings: Vec<_> = args
        .iter()
        .map(|s| s.to_string_lossy().into_owned())
        .collect();
    spacelens::engine::configure(spacelens::engine::parse_options(&strings));
    let mut path = None;
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--engine" || args[i] == "--workers" {
            i += 2;
        } else {
            path = Some(std::path::PathBuf::from(&args[i]));
            i += 1;
        }
    }
    std::process::exit(spacelens::ui::run(path, true));
}
