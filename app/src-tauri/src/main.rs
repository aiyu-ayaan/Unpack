fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // No arguments opens the window; any argument is a command (`unpack install file.deb`).
    #[cfg(feature = "gui")]
    if args.is_empty() {
        return unpack_lib::run();
    }
    std::process::exit(unpack_lib::cli::main(args));
}
