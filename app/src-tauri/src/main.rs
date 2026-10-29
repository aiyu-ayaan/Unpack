fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // No arguments opens the window; any argument is a command (`unpack install file.deb`).
    // `open <file>` is what "Open with Unpack" starts: the window, with that file in Install.
    #[cfg(feature = "gui")]
    match args.split_first() {
        None => return unpack_lib::run(None),
        Some((cmd, rest)) if cmd == "open" && !rest.is_empty() => return unpack_lib::run(Some(rest.join(" "))),
        _ => {}
    }
    std::process::exit(unpack_lib::cli::main(args));
}
