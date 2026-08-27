fn main() {
    if let Err(error) = dvandva_v4::cli::run() {
        dvandva_v4::cli::print_error(&error);
        std::process::exit(1);
    }
}
