fn main() {
    if let Err(err) = cockpit_web::run_from_env() {
        eprintln!("{err}");
        std::process::exit(1);
    }
}
