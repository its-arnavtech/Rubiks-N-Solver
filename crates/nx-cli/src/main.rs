//! `nx` command-line entry point. Subcommands: `orbits` (M2.5); discover, verify-library,
//! solve and bench are added in M3–M4.

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
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Command::Orbits { n, list } => print_orbits(n, list),
    }
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
