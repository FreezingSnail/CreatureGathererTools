fn main() {
    if let Err(error) = creaturevm::run() {
        eprintln!("{error:#}");
        std::process::exit(creaturevm::exit_code(&error));
    }
}
