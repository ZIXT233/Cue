#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let mut args = std::env::args();
    let _ = args.next();
    if args.next().as_deref() == Some("--askpass") {
        if let Err(error) = cue_lib::askpass::run() {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return;
    }
    cue_lib::run();
}
