fn main() {
    if let Err(error) = commitlint_rust::cli::run(std::env::args().skip(1).collect()) {
        eprintln!("commitlint: {error}");
        std::process::exit(1)
    }
}
