//! `cargo xtask <command>`: cross-platform build orchestration (docs/03 §5.2).

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::{Duration, SystemTime};

const USAGE: &str = "usage: cargo xtask <doctor | wasm [--watch] | dev | ci>";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

/// Commands like `pnpm` are `.cmd` shims on Windows and must go through `cmd /C`.
fn tool(name: &str) -> Command {
    if cfg!(windows) && matches!(name, "pnpm" | "npm" | "npx") {
        let mut c = Command::new("cmd");
        c.args(["/C", name]);
        c
    } else {
        Command::new(name)
    }
}

fn run(mut cmd: Command) -> Result<(), String> {
    let shown = format!("{cmd:?}");
    let status = cmd
        .current_dir(root())
        .status()
        .map_err(|e| format!("{shown}: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{shown} failed with {status}"))
    }
}

fn output(name: &str, args: &[&str]) -> Option<String> {
    let out = tool(name).args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Version of the `wasm-bindgen` crate pinned in Cargo.lock.
fn locked_wasm_bindgen() -> Option<String> {
    let lock = std::fs::read_to_string(root().join("Cargo.lock")).ok()?;
    let mut lines = lock.lines();
    while let Some(line) = lines.next() {
        if line == "name = \"wasm-bindgen\"" {
            return lines
                .next()?
                .strip_prefix("version = \"")?
                .strip_suffix('"')
                .map(str::to_string);
        }
    }
    None
}

fn doctor() -> Result<(), String> {
    let mut ok = true;
    let mut check = |label: &str, value: Option<String>, hint: &str| match value {
        Some(v) => println!("  ✔ {label}: {v}"),
        None => {
            ok = false;
            println!("  ✘ {label}: missing. {hint}");
        }
    };
    check(
        "rustc",
        output("rustc", &["--version"]),
        "install Rust via rustup",
    );
    let target = output("rustup", &["target", "list", "--installed"])
        .filter(|s| s.lines().any(|l| l == "wasm32-unknown-unknown"))
        .map(|_| "wasm32-unknown-unknown".to_string());
    check(
        "wasm target",
        target,
        "run: rustup target add wasm32-unknown-unknown",
    );
    let locked = locked_wasm_bindgen();
    let installed = output("wasm-bindgen", &["--version"]);
    let hint = format!(
        "run: cargo install wasm-bindgen-cli --version {}",
        locked.as_deref().unwrap_or("<see Cargo.lock>")
    );
    let matched = match (&installed, &locked) {
        (Some(i), Some(l)) if i.ends_with(l.as_str()) => Some(i.clone()),
        _ => None,
    };
    check("wasm-bindgen-cli (matching Cargo.lock)", matched, &hint);
    check(
        "node",
        output("node", &["--version"]),
        "install Node.js 22+",
    );
    check(
        "pnpm",
        output("pnpm", &["--version"]),
        "run: npm install -g pnpm",
    );
    if ok {
        Ok(())
    } else {
        Err("doctor found problems".into())
    }
}

fn build_wasm() -> Result<(), String> {
    let mut cargo = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()));
    cargo.args([
        "build",
        "-p",
        "nx-wasm",
        "--target",
        "wasm32-unknown-unknown",
        "--release",
    ]);
    run(cargo)?;
    let wasm = root().join("target/wasm32-unknown-unknown/release/nx_wasm.wasm");
    let out_dir = root().join("web/src/engine/pkg");
    let mut bindgen = tool("wasm-bindgen");
    bindgen
        .arg(&wasm)
        .arg("--out-dir")
        .arg(&out_dir)
        .args(["--target", "web"]);
    run(bindgen)?;
    println!("wasm → {}", out_dir.display());
    Ok(())
}

fn newest_source_mtime(dir: &Path) -> SystemTime {
    let mut newest = SystemTime::UNIX_EPOCH;
    let Ok(entries) = std::fs::read_dir(dir) else {
        return newest;
    };
    for e in entries.flatten() {
        let p = e.path();
        let t = if p.is_dir() {
            newest_source_mtime(&p)
        } else if p.extension().is_some_and(|x| x == "rs" || x == "toml") {
            e.metadata().and_then(|m| m.modified()).unwrap_or(newest)
        } else {
            continue;
        };
        newest = newest.max(t);
    }
    newest
}

fn watch_wasm() -> ! {
    let crates = root().join("crates");
    let mut last = SystemTime::UNIX_EPOCH;
    loop {
        let now = newest_source_mtime(&crates);
        if now > last {
            last = now;
            if let Err(e) = build_wasm() {
                eprintln!("wasm build failed: {e}");
            }
        }
        std::thread::sleep(Duration::from_millis(800));
    }
}

fn dev() -> Result<(), String> {
    build_wasm()?;
    std::thread::spawn(|| watch_wasm());
    let mut vite = tool("pnpm");
    vite.args(["--dir", "web", "dev"]);
    run(vite)
}

fn ci() -> Result<(), String> {
    let cargo = || Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()));
    let mut c = cargo();
    c.args(["fmt", "--all", "--check"]);
    run(c)?;
    let mut c = cargo();
    c.args([
        "clippy",
        "--workspace",
        "--all-targets",
        "--",
        "-D",
        "warnings",
    ]);
    run(c)?;
    let mut c = cargo();
    c.args(["test", "--workspace"]);
    run(c)?;
    build_wasm()?;
    for script in ["check", "test", "build"] {
        let mut c = tool("pnpm");
        c.args(["--dir", "web", "run", script]);
        run(c)?;
    }
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["doctor"] => doctor(),
        ["wasm"] => build_wasm(),
        ["wasm", "--watch"] => watch_wasm(),
        ["dev"] => dev(),
        ["ci"] => ci(),
        _ => Err(USAGE.into()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("xtask: {e}");
            ExitCode::FAILURE
        }
    }
}
