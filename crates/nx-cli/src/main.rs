//! `nx` command-line entry point. Subcommands: `orbits` (M2.5); discover, verify-library,
//! solve and bench are added in M3–M4.

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use clap::{Parser, Subcommand};
use nx_sim::{OrbitKind, orbits};

#[derive(Parser)]
#[command(
    name = "nx",
    version,
    about = "NxN cube tools: orbits, macro library, solving"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Print the orbit types and counts of an NxN cube.
    Orbits {
        /// Cube size (N >= 2).
        n: u32,
        /// Also list every orbit with its id and indices.
        #[arg(long)]
        list: bool,
    },
    /// Discover macros, expand them to actions and write the library (canonical JSON).
    Discover {
        #[arg(long, default_value = nx_macro::LIBRARY_PATH)]
        out: PathBuf,
        /// Don't write; fail unless the regenerated library equals the file at --out.
        #[arg(long)]
        check: bool,
    },
    /// Solve a scrambled or random cube and verify the solution.
    Solve {
        /// Use the deterministic baseline solver (the only solver in Rust).
        #[arg(long)]
        baseline: bool,
        #[arg(long)]
        n: u32,
        #[arg(long, default_value_t = 0)]
        seed: u64,
        /// Start from a uniform random state instead of a move scramble.
        #[arg(long)]
        random_state: bool,
        /// Scramble length (default 20·N).
        #[arg(long)]
        len: Option<usize>,
        /// Solve this many cubes (seeds seed..seed+count) and print aggregate stats.
        #[arg(long, default_value_t = 1)]
        count: u64,
        /// Write the SolveResult JSON here (single solve only).
        #[arg(long)]
        json: Option<PathBuf>,
        #[arg(long, default_value = nx_macro::LIBRARY_PATH)]
        library: PathBuf,
    },
    /// Check the library: purity, invariance and coverage (ARCHITECTURE §6.3).
    VerifyLibrary {
        #[arg(long, default_value = nx_macro::LIBRARY_PATH)]
        path: PathBuf,
        /// Check every instance for N up to this size.
        #[arg(long, default_value_t = 18)]
        max_full_n: u32,
        /// Spot-check sizes (sampled instances).
        #[arg(long, value_delimiter = ',', default_value = "31,64,101")]
        spot: Vec<u32>,
    },
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Command::Orbits { n, list } => print_orbits(n, list),
        Command::Discover { out, check } => discover(&out, check),
        Command::VerifyLibrary {
            path,
            max_full_n,
            spot,
        } => verify_library(&path, max_full_n, spot),
        Command::Solve {
            baseline,
            n,
            seed,
            random_state,
            len,
            count,
            json,
            library,
        } => {
            if !baseline {
                eprintln!(
                    "error: only --baseline is available in Rust; the NN solver is `just solve N SEED nn`"
                );
                return ExitCode::FAILURE;
            }
            solve(n, seed, random_state, len, count, json.as_deref(), &library)
        }
    }
}

fn solve(
    n: u32,
    seed: u64,
    random: bool,
    len: Option<usize>,
    count: u64,
    json: Option<&std::path::Path>,
    library: &std::path::Path,
) -> ExitCode {
    if n < 2 {
        eprintln!("error: N must be at least 2");
        return ExitCode::FAILURE;
    }
    let lib = match nx_macro::Library::load(library).and_then(|l| nx_solve::SolverLib::new(&l)) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    let len = len.unwrap_or(20 * n as usize);
    let (mut raw, mut cancelled, mut total_ms, mut worst_ms) = (0u64, 0u64, 0f64, 0f64);
    for s in seed..seed + count {
        let cube = if random {
            nx_sim::random_state(n, s)
        } else {
            nx_sim::Cube::scramble(n, len, s).0
        };
        let out = match nx_solve::solve_baseline(&cube, &lib) {
            Ok(o) => o,
            Err(e) => {
                eprintln!("error: N={n} seed={s}: {e}");
                return ExitCode::FAILURE;
            }
        };
        let r = &out.result;
        raw += r.raw_len;
        cancelled += r.cancelled_len;
        total_ms += r.timings_ms.total;
        worst_ms = worst_ms.max(r.timings_ms.total);
        if count == 1 {
            println!(
                "N={n} seed={s} {}: verified, {} raw / {} cancelled moves, {} orbits, {} rounds",
                if random { "random state" } else { "scramble" },
                r.raw_len,
                r.cancelled_len,
                r.stats.orbits_total,
                r.stats.rounds
            );
            let t = &r.timings_ms;
            println!(
                "time: total {:.1} ms (extract {:.1}, plan {:.1}, emit {:.1}, verify {:.1})",
                t.total, t.extract, t.plan, t.emit, t.verify
            );
            if let Some(path) = json {
                let text = serde_json::to_string(r).expect("serializable");
                if let Err(e) = std::fs::write(path, text) {
                    eprintln!("error: {}: {e}", path.display());
                    return ExitCode::FAILURE;
                }
                println!("wrote {}", path.display());
            }
        }
    }
    if count > 1 {
        let c = count as f64;
        println!(
            "N={n} {count}/{count} verified ({}): mean {:.1} raw / {:.1} cancelled moves, mean {:.2} ms, max {:.2} ms",
            if random { "random states" } else { "scrambles" },
            raw as f64 / c,
            cancelled as f64 / c,
            total_ms / c,
            worst_ms
        );
    }
    ExitCode::SUCCESS
}

fn git_commit() -> String {
    std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

fn discover(out: &std::path::Path, check: bool) -> ExitCode {
    let start = Instant::now();
    let (lib, reports) = nx_macro::library::build(&git_commit());
    println!(
        "{:<11} {:>8} {:>8} {:>8} {:>9} {:>6}  kinds",
        "type", "found", "macros", "actions", "mean cost", "max"
    );
    for r in &reports {
        println!(
            "{:<11} {:>8} {:>8} {:>8} {:>9.2} {:>6}  {:?}",
            r.kind.name(),
            r.macros_found,
            r.macros_used,
            r.actions,
            r.mean_cost,
            r.max_cost,
            r.by_kind
        );
    }
    println!(
        "sha256 {}  ({:.1} s)",
        lib.sha256,
        start.elapsed().as_secs_f64()
    );
    if check {
        return match nx_macro::Library::load(out) {
            Ok(old) if old.sha256 == lib.sha256 => {
                println!("{} is up to date", out.display());
                ExitCode::SUCCESS
            }
            Ok(old) => {
                eprintln!(
                    "error: {} has sha256 {}, regenerated {}",
                    out.display(),
                    old.sha256,
                    lib.sha256
                );
                ExitCode::FAILURE
            }
            Err(e) => {
                eprintln!("error: {e}");
                ExitCode::FAILURE
            }
        };
    }
    match lib.save(out) {
        Ok(()) => {
            println!("wrote {}", out.display());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn verify_library(path: &std::path::Path, max_full_n: u32, spot: Vec<u32>) -> ExitCode {
    let lib = match nx_macro::Library::load(path) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    let opts = nx_macro::verify::VerifyOptions {
        max_full_n,
        spot_ns: spot,
        ..Default::default()
    };
    let start = Instant::now();
    let reports = nx_macro::verify::verify(&lib, &opts);
    let mut failed = 0;
    println!(
        "{:<11} {:>8} {:>10} {:>12} {:>8}  sizes",
        "type", "actions", "instances", "checks", "seconds"
    );
    for r in &reports {
        println!(
            "{:<11} {:>8} {:>10} {:>12} {:>8.1}  {:?}  {}",
            r.kind.name(),
            r.actions,
            r.instances,
            r.checks,
            r.seconds,
            r.sizes,
            if r.failures.is_empty() { "OK" } else { "FAIL" }
        );
        for f in r.failures.iter().take(10) {
            println!("    {f}");
        }
        if r.failures.len() > 10 {
            println!("    ... {} more", r.failures.len() - 10);
        }
        failed += r.failures.len();
    }
    println!(
        "library {}  ({:.1} s)",
        lib.sha256,
        start.elapsed().as_secs_f64()
    );
    if failed > 0 {
        eprintln!("error: {failed} verification failures");
        return ExitCode::FAILURE;
    }
    println!("purity, invariance and coverage: OK");
    ExitCode::SUCCESS
}

fn print_orbits(n: u32, list: bool) -> ExitCode {
    if n < 2 {
        eprintln!("error: N must be at least 2");
        return ExitCode::FAILURE;
    }
    let start = Instant::now();
    let os = orbits(n);
    let elapsed = start.elapsed();
    println!(
        "N = {n}: {} orbits, {} stickers",
        os.len(),
        nx_sim::sticker_count(n)
    );
    println!(
        "{:<12} {:>7} {:>9} {:>7}",
        "type", "type_id", "orbits", "slots"
    );
    for kind in OrbitKind::ALL {
        let count = os.iter().filter(|o| o.kind == kind).count();
        if count == 0 {
            continue;
        }
        let type_id = kind.type_id().map_or("-".to_string(), |t| t.to_string());
        println!(
            "{:<12} {:>7} {:>9} {:>7}",
            kind.name(),
            type_id,
            count,
            kind.slot_count()
        );
    }
    if list {
        println!();
        println!("{:>6}  {:<12} indices", "id", "type");
        for o in &os {
            let idx = match o.kind {
                OrbitKind::Wing => format!("p={}", o.a),
                OrbitKind::XCenter | OrbitKind::PlusCenter => format!("a={}", o.a),
                OrbitKind::ObliqueA | OrbitKind::ObliqueB => format!("a={} b={}", o.a, o.b),
                _ => String::new(),
            };
            println!("{:>6}  {:<12} {idx}", o.id, o.kind.name());
        }
    }
    eprintln!("(computed in {:.1} ms)", elapsed.as_secs_f64() * 1e3);
    ExitCode::SUCCESS
}
