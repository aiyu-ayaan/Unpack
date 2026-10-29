//! `unpack <command>`: the window's analyze → plan → install flow in a terminal, on the same database.
use crate::plan::{Plan, Uninstall};
use crate::store::{App, Store};
use crate::{analyze, catalog, fix, library, ops, plan as plans, runner, sandbox, system, transfer, updates, upgrades, util};
use std::io::{IsTerminal, Write};
use std::sync::Mutex;

const HELP: &str = "Unpack: install anything on Linux, and remove it later.

Usage: unpack [command] [options]

With no command the desktop app opens (the command-line-only build prints this help).

Commands:
  check <input>        Show the plan for a file, link, git address or curl/wget command; nothing runs
  install <input>      Show the plan, ask, install, and track it
  search <words>       Find apps on Flathub, the Snap Store and your distro's repositories
  get <source> <id>    Install one search result (source: flathub, snap, apt, dnf, zypper, pacman, apk)
  list                 Apps installed with Unpack (--all: removed ones too)
  updates              What can be updated: system packages, Flatpaks, snaps, and apps from GitHub releases
  upgrade [name...]    Update those packages (all of them with no names)
  remove <id|name>     Uninstall a tracked app (--purge: also its settings and cache folders)
  fix <id|name>        Find what keeps an app from starting and repair it
  export [file]        Save the list of installed apps (default ~/unpack-apps.json) to take to another computer
  import <file>        Install the apps of an exported list again, asking for each
  scan                 Every package the system's package managers report
  history [id|name]    What Unpack did, newest first
  system               What Unpack detected about this computer
  version              Print the version

Options:
  -y, --yes            Do not ask before installing or removing
      --root           Run the main step as administrator, when the plan allows it
      --allow-http     Accept a plain http:// link
      --sandbox        With check or install: first try a script in a sandbox (bubblewrap) and show what it did
      --purge          With remove: also delete leftover settings, cache and data folders
      --all            With list: include removed apps
      --json           Machine-readable output

Exit codes: 0 done, 1 error, 2 the plan can't be run, 3 declined.";

#[derive(Default)]
struct Opts {
    yes: bool,
    root: bool,
    http: bool,
    purge: bool,
    sandbox: bool,
    all: bool,
    json: bool,
    args: Vec<String>,
}

fn parse(args: &[String]) -> Result<Opts, String> {
    let mut o = Opts::default();
    for a in args {
        match a.as_str() {
            "-y" | "--yes" => o.yes = true,
            "--root" => o.root = true,
            "--allow-http" => o.http = true,
            "--purge" => o.purge = true,
            "--sandbox" => o.sandbox = true,
            "--all" => o.all = true,
            "--json" => o.json = true,
            s if s.starts_with('-') && s.len() > 1 => return Err(format!("unknown option {s}. See `unpack --help`.")),
            _ => o.args.push(a.clone()),
        }
    }
    Ok(o)
}

pub fn main(args: Vec<String>) -> i32 {
    match run(&args) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e}");
            1
        }
    }
}

fn json<T: serde::Serialize>(v: &T) -> Result<(), String> {
    println!("{}", serde_json::to_string_pretty(v).map_err(|e| e.to_string())?);
    Ok(())
}

fn store() -> Result<Mutex<Store>, String> {
    Ok(Mutex::new(Store::open(&util::data_dir().join("unpack.db"))?))
}

/// The one argument a command takes; for `install` and `check` the rest are joined, so an unquoted
/// `curl -fsSL … | sh` still arrives whole.
fn input(o: &Opts, what: &str) -> Result<String, String> {
    if o.args.is_empty() {
        return Err(format!("{what} needs an argument. See `unpack --help`."));
    }
    Ok(o.args.join(" "))
}

fn run(args: &[String]) -> Result<i32, String> {
    let Some((cmd, rest)) = args.split_first() else {
        println!("{HELP}");
        return Ok(0);
    };
    let o = parse(rest)?;
    match cmd.as_str() {
        "-h" | "--help" | "help" => println!("{HELP}"),
        "-V" | "--version" | "version" => println!("unpack {}", updates::version()),
        "system" => {
            let s = system::detect();
            if o.json {
                json(&s)?;
            } else {
                println!("Distro      {} ({:?} family)", s.distro, s.family);
                println!("Arch        {}{}", s.arch, if s.musl { " (musl)" } else { "" });
                println!("Native      {}", s.native.map(|n| format!("{n:?}").to_lowercase()).unwrap_or_else(|| "none".into()));
                println!("Managers    {}", s.managers.join(", "));
                println!("Runtimes    {}", if s.runtimes.is_empty() { "none".into() } else { s.runtimes.join(", ") });
                println!("Missing     {}", if s.missing_helpers.is_empty() { "nothing".into() } else { s.missing_helpers.join(", ") });
                if s.immutable {
                    println!("Read-only base system");
                }
                if s.is_root {
                    println!("Running as root: user-scope apps go to root's home");
                }
            }
        }
        "check" | "analyze" => {
            let plan = check(&input(&o, cmd)?, &o)?;
            return Ok(if plan.supported { 0 } else { 2 });
        }
        // `open` is the window's "Open with Unpack" entry; without a window it is install.
        "install" | "open" => return install(&input(&o, cmd)?, &o),
        "search" => {
            let found = catalog::search(&input(&o, cmd)?, &system::detect())?;
            if o.json {
                json(&found)?;
            } else if found.is_empty() {
                println!("Nothing found.");
            } else {
                for e in &found {
                    println!("{:<8} {:<40} {}", e.source, e.id, e.summary);
                }
                println!("Install one with `unpack get <source> <id>`.");
            }
        }
        "get" => {
            let [source, id] = o.args.as_slice() else { return Err("get needs a source and an id, as `unpack search` lists them.".into()) };
            let mut plan = plans::plan_repo(source, id, &system::detect());
            plan.id = analyze::new_id();
            if o.json { json(&plan)?; } else { show(&plan); }
            return run_plan(plan, &o);
        }
        "list" => {
            let apps: Vec<App> = store()?.into_inner().map_err(|e| e.to_string())?.list()?.into_iter().filter(|a| o.all || a.status == "installed").collect();
            if o.json {
                json(&apps)?;
            } else if apps.is_empty() {
                println!("Nothing installed with Unpack yet.");
            } else {
                for a in &apps {
                    println!("{:>4}  {:<28} {:<14} {:<10} {:<9} {}", a.id, a.name, a.version.as_deref().unwrap_or("-"), a.kind, a.status, how(&a.uninstall));
                }
            }
        }
        "remove" | "uninstall" => return remove(&o),
        "updates" => {
            let all = upgrades::list(&store()?.into_inner().map_err(|e| e.to_string())?.list()?, &system::detect());
            if o.json {
                json(&all)?;
            } else if all.is_empty() {
                println!("Everything is up to date.");
            } else {
                for u in &all {
                    println!("{:<9} {:<36} {:<18} -> {}", u.manager, u.name, if u.current.is_empty() { "?" } else { &u.current }, u.latest);
                }
            }
        }
        "upgrade" => return upgrade(&o),
        "export" => {
            let path = o.args.first().map(std::path::PathBuf::from).unwrap_or_else(transfer::default_path);
            let n = transfer::write(&store()?.into_inner().map_err(|e| e.to_string())?.list()?, &path)?;
            println!("Saved {n} apps to {}.", path.display());
        }
        "import" => return import(&o),
        "fix" => {
            let key = o.args.first().ok_or("fix needs an app id or name. See `unpack list`.")?;
            let app = find(&store()?.into_inner().map_err(|e| e.to_string())?, key, false)?;
            let mut plan = fix::plan(&app, &system::detect());
            plan.id = analyze::new_id();
            if o.json { json(&plan)?; } else { show(&plan); }
            return run_plan(plan, &o);
        }
        "scan" => {
            let pkgs = library::scan();
            if o.json {
                json(&pkgs)?;
            } else {
                for p in &pkgs {
                    println!("{:<8} {:<40} {}", p.manager, p.name, p.version);
                }
            }
        }
        "history" => {
            let db = store()?.into_inner().map_err(|e| e.to_string())?;
            let id = match o.args.first() {
                Some(a) => Some(find(&db, a, true)?.id),
                None => None,
            };
            let events = db.events(id)?;
            if o.json {
                json(&events)?;
            } else {
                for e in &events {
                    println!("{} {:<5} {}", e.ts, e.level, e.message);
                }
            }
        }
        other => return Err(format!("unknown command {other:?}. See `unpack --help`.")),
    }
    Ok(0)
}

fn how(u: &Uninstall) -> String {
    match u {
        Uninstall::Manager { manager, .. } => format!("via {manager}"),
        Uninstall::Files { paths } => format!("{} files tracked", paths.len()),
        Uninstall::Traced => "traced".into(),
        Uninstall::Manual { .. } => "remove manually".into(),
    }
}

/// Download progress on stderr, only in a terminal.
fn progress(done: u64, total: Option<u64>) -> bool {
    let err = std::io::stderr();
    if err.is_terminal() && done > 0 {
        let mb = |b: u64| b as f64 / 1_048_576.0;
        let line = match total {
            Some(t) if t > 0 => format!("\rDownloading {:.1} of {:.1} MB ({}%)", mb(done), mb(t), done * 100 / t),
            _ => format!("\rDownloading {:.1} MB", mb(done)),
        };
        let _ = write!(err.lock(), "{line}\x1b[K");
    }
    true
}

fn check(text: &str, o: &Opts) -> Result<Plan, String> {
    let plan = analyze::analyze(text, &system::detect(), o.http, &progress);
    if std::io::stderr().is_terminal() {
        eprint!("\r\x1b[K");
    }
    let plan = plan?;
    if o.json {
        json(&plan)?;
    } else {
        show(&plan);
    }
    if o.sandbox {
        let r = sandbox::preview(&plan)?;
        if o.json {
            json(&r)?;
        } else {
            println!("Sandbox run (no network, read-only system, throwaway home): {}", match (r.timed_out, r.code) { (true, _) => "stopped after 2 minutes".into(), (_, Some(c)) => format!("exit {c}"), _ => "killed".into() });
            for l in &r.output {
                println!("  | {l}");
            }
            println!("Files it created in home: {}", if r.files.is_empty() { "none".into() } else { r.files.join(", ") });
        }
    }
    Ok(plan)
}

fn show(p: &Plan) {
    println!("{}{}  [{}]", p.name, p.version.as_deref().map(|v| format!(" {v}")).unwrap_or_default(), p.kind);
    println!("Source   {}", p.source);
    if let Some(h) = &p.sha256 {
        println!("SHA-256  {h}");
    }
    for w in &p.warnings {
        println!("{}  {w}", if p.supported { "warning:" } else { "cannot:" });
    }
    for a in &p.advice {
        println!("advice:  {a}");
    }
    if !p.steps.is_empty() {
        println!("Steps:");
        for (i, s) in p.steps.iter().enumerate() {
            println!("  {}. {}{}\n     {}", i + 1, s.label, if s.needs_root { " (admin)" } else { "" }, s.display);
        }
    }
    if let Some(pre) = &p.preview {
        let lines: Vec<&str> = pre.lines().collect();
        println!("Contents ({} lines{}):", lines.len(), if lines.len() > 40 { ", first 40" } else { "" });
        for l in lines.iter().take(40) {
            println!("  | {l}");
        }
    }
}

fn ask(question: &str, o: &Opts) -> Result<bool, String> {
    if o.yes {
        return Ok(true);
    }
    if !std::io::stdin().is_terminal() {
        return Err("not a terminal, so Unpack can't ask. Pass --yes to go ahead.".into());
    }
    print!("{question} [y/N] ");
    let _ = std::io::stdout().flush();
    let mut line = String::new();
    std::io::stdin().read_line(&mut line).map_err(|e| e.to_string())?;
    Ok(matches!(line.trim(), "y" | "Y" | "yes"))
}

fn emitter(json: bool) -> impl Fn(&str, &str) {
    move |stream: &str, line: &str| match stream {
        "out" if !json => println!("{line}"),
        "info" => eprintln!(":: {line}"),
        "warn" => eprintln!("warning: {line}"),
        _ => eprintln!("{line}"),
    }
}

fn install(text: &str, o: &Opts) -> Result<i32, String> {
    run_plan(check(text, o)?, o)
}

/// Ask, run and record a plan the user has been shown.
fn run_plan(plan: Plan, o: &Opts) -> Result<i32, String> {
    if !plan.supported {
        return Ok(2);
    }
    if !ask(&format!("{}?", plan.title), o)? {
        eprintln!("Nothing was installed.");
        return Ok(3);
    }
    runner::TERMINAL_SUDO.store(std::io::stdin().is_terminal(), std::sync::atomic::Ordering::Relaxed);
    let db = store()?;
    let r = ops::install(&plan, o.root, &system::detect(), &db, &emitter(o.json))?;
    if o.json {
        json(&r)?;
    }
    for n in &r.notes {
        eprintln!("note: {n}");
    }
    match (&r.error, r.app_id) {
        (None, Some(id)) => {
            if !o.json {
                println!("Installed {} (id {id}). Remove it with `unpack remove {id}`.", plan.name);
            }
            Ok(0)
        }
        // A fix or an in-place update records no new app.
        (None, None) => Ok(0),
        (Some(e), _) => Err(e.clone()),
    }
}

/// Package updates go in one plan; a newer release file of a tracked app is analyzed and installed in its place.
fn upgrade(o: &Opts) -> Result<i32, String> {
    let db = store()?;
    let apps = db.lock().map_err(|e| e.to_string())?.list()?;
    let all = upgrades::list(&apps, &system::detect());
    let chosen: Vec<&upgrades::Upgrade> = all.iter().filter(|u| o.args.is_empty() || o.args.iter().any(|a| a == &u.name)).collect();
    if chosen.is_empty() {
        println!("Nothing to update.");
        return Ok(0);
    }
    let picks: Vec<upgrades::Pick> = chosen.iter().filter(|u| u.manager != "download").map(|u| upgrades::Pick { manager: u.manager.clone(), name: u.name.clone(), user: u.user }).collect();
    let mut code = 0;
    if !picks.is_empty() {
        let mut plan = upgrades::plan(&picks, &system::detect());
        plan.id = analyze::new_id();
        if o.json { json(&plan)?; } else { show(&plan); }
        code = run_plan(plan, o)?;
    }
    for u in chosen.iter().filter(|u| u.manager == "download") {
        let app = apps.iter().find(|a| Some(a.id) == u.app_id).ok_or("record vanished")?;
        let Some((_, _, url)) = upgrades::newer_download(app) else { continue };
        let mut plan = check(&url, o)?;
        plan.track = plans::Track::Replace(app.id);
        code = code.max(run_plan(plan, o)?);
    }
    Ok(code)
}

fn import(o: &Opts) -> Result<i32, String> {
    let path = o.args.first().ok_or("import needs the file `unpack export` wrote.")?;
    let sys = system::detect();
    let mut code = 0;
    for i in transfer::read(std::path::Path::new(path), &sys)? {
        let plan = match &i.way {
            transfer::Way::Store { source, id } => {
                let mut p = plans::plan_repo(source, id, &sys);
                p.id = analyze::new_id();
                show(&p);
                p
            }
            transfer::Way::Input { input } => match check(input, o) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("{}: {e}", i.item.name);
                    code = 1;
                    continue;
                }
            },
            transfer::Way::None { why } => {
                println!("{}: skipped. {why}", i.item.name);
                continue;
            }
        };
        match run_plan(plan, o) {
            Ok(c) => code = code.max(c),
            Err(e) => {
                eprintln!("{}: {e}", i.item.name);
                code = 1;
            }
        }
    }
    Ok(code)
}

/// An app by id, or by name when exactly one matches.
fn find(db: &Store, key: &str, any_status: bool) -> Result<App, String> {
    if let Ok(id) = key.parse::<i64>() {
        return db.get(id);
    }
    let all: Vec<App> = db.list()?.into_iter().filter(|a| a.name.eq_ignore_ascii_case(key)).collect();
    let live: Vec<&App> = all.iter().filter(|a| a.status == "installed").collect();
    match live.as_slice() {
        [one] => Ok((*one).clone()),
        [_, _, ..] => Err(format!("more than one app is called {key}; use its id from `unpack list`.")),
        [] if all.is_empty() => Err(format!("nothing called {key} was installed with Unpack. See `unpack list`.")),
        [] if any_status => Ok(all[0].clone()),
        [] => Err(format!("{key} is already removed. Use its id from `unpack list --all`.")),
    }
}

fn remove(o: &Opts) -> Result<i32, String> {
    let key = o.args.first().ok_or("remove needs an app id or name. See `unpack list`.")?;
    let db = store()?;
    // --purge also takes an app that is already removed, to clean up after it.
    let app = find(&*db.lock().map_err(|e| e.to_string())?, key, o.purge)?;
    let emit = emitter(o.json);
    if app.status == "installed" {
        if let Uninstall::Manual { note } = &app.uninstall {
            return Err(format!("{} can't be removed automatically. {note}", app.name));
        }
        if !ask(&format!("Remove {} ({})?", app.name, how(&app.uninstall)), o)? {
            return Ok(3);
        }
        runner::TERMINAL_SUDO.store(std::io::stdin().is_terminal(), std::sync::atomic::Ordering::Relaxed);
        let r = ops::uninstall(app.id, &system::detect(), &db, &emit)?;
        if let Some(e) = r.error {
            return Err(e);
        }
        if !o.json {
            println!("Removed {}.", app.name);
        }
    }
    let left = ops::leftovers(&app);
    let gone = if o.purge && !left.is_empty() {
        ops::remove_leftovers(app.id, &db, &emit)?
    } else {
        if !left.is_empty() && !o.json {
            println!("It left these folders behind:");
            for p in &left {
                println!("  {}", p.display());
            }
            println!("Delete them with `unpack remove {} --purge`.", app.id);
        }
        vec![]
    };
    if o.json {
        json(&serde_json::json!({ "ok": true, "app_id": app.id, "purged": gone, "leftovers": if o.purge { vec![] } else { left } }))?;
    }
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(s: &[&str]) -> Vec<String> {
        s.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn parses_flags_and_rejects_unknown_ones() {
        let o = parse(&args(&["file.deb", "-y", "--purge", "--json"])).unwrap();
        assert!(o.yes && o.purge && o.json && !o.root);
        assert_eq!(o.args, ["file.deb"]);
        assert!(parse(&args(&["--force"])).is_err());
        assert_eq!(main(args(&["frobnicate"])), 1);
        assert_eq!(main(args(&["--version"])), 0);
    }

    #[test]
    fn finds_apps_by_id_or_unique_name() {
        let db = Store::memory().unwrap();
        let a = db.add_app("Tool", None, "x", "x", None, &Uninstall::Traced).unwrap();
        let b = db.add_app("dup", None, "x", "x", None, &Uninstall::Traced).unwrap();
        db.add_app("dup", None, "x", "x", None, &Uninstall::Traced).unwrap();
        assert_eq!(find(&db, "tool", false).unwrap().id, a);
        assert_eq!(find(&db, &b.to_string(), false).unwrap().id, b);
        assert!(find(&db, "dup", false).is_err());
        db.set_status(a, "removed").unwrap();
        assert!(find(&db, "Tool", false).unwrap_err().contains("already removed"));
        assert_eq!(find(&db, "Tool", true).unwrap().id, a);
        assert!(find(&db, "nope", false).is_err());
    }
}
