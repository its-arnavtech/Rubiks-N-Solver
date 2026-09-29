//! `rgraph`: native CLI (docs/03 §5.2).

use std::process::ExitCode;
use std::time::Instant;

use rg_solve::{SolveOptions, Solver};

const USAGE: &str = "usage:
  rgraph apply    --size <N> \"<alg>\"
  rgraph scramble --size <N> [--seed <u64>]
  rgraph solve    --size <N> \"<scramble alg>\" [--target <len>] [--time <ms>]
  rgraph bench    --size <N> [--count <k>] [--target <len>] [--time <ms>]";

fn print_net(cube: &rg_cube::Cube) {
    let n = usize::from(cube.n());
    let f = cube.facelets();
    let row = |face: usize, r: usize| -> String {
        (0..n)
            .map(|c| rg_cube::FACE_NAMES[usize::from(f[face * n * n + r * n + c])])
            .collect()
    };
    let pad = " ".repeat(n + 1);
    for r in 0..n {
        println!("{pad}{}", row(0, r));
    }
    for r in 0..n {
        println!("{} {} {} {}", row(4, r), row(2, r), row(1, r), row(5, r));
    }
    for r in 0..n {
        println!("{pad}{}", row(3, r));
    }
    println!("solved: {}", cube.is_solved());
}

struct Args {
    size: u8,
    seed: u64,
    count: u64,
    opts: SolveOptions,
    positional: Vec<String>,
}

fn parse(args: &[String]) -> Result<Args, String> {
    let mut a = Args {
        size: 3,
        seed: 1,
        count: 100,
        opts: SolveOptions::default(),
        positional: vec![],
    };
    let mut it = args.iter().skip(1);
    while let Some(arg) = it.next() {
        let mut value = |name: &str| it.next().ok_or(format!("{name} needs a value"));
        match arg.as_str() {
            "--size" => a.size = value("--size")?.parse().map_err(|_| "bad --size")?,
            "--seed" => a.seed = value("--seed")?.parse().map_err(|_| "bad --seed")?,
            "--count" => a.count = value("--count")?.parse().map_err(|_| "bad --count")?,
            "--target" => {
                a.opts.target_length = value("--target")?.parse().map_err(|_| "bad --target")?
            }
            "--time" => a.opts.max_time_ms = value("--time")?.parse().map_err(|_| "bad --time")?,
            _ => a.positional.push(arg.clone()),
        }
    }
    Ok(a)
}

fn run(args: &[String]) -> Result<(), String> {
    let a = parse(args)?;
    let mut cube = rg_cube::Cube::new(a.size).map_err(|e| e.to_string())?;
    let t0 = Instant::now();
    let now = move || t0.elapsed().as_secs_f64() * 1000.0;
    match a.positional.first().map(String::as_str) {
        Some("apply") => {
            cube.apply_alg(a.positional.get(1).ok_or(USAGE)?)
                .map_err(|e| e.to_string())?;
            print_net(&cube);
        }
        Some("scramble") => {
            let moves =
                rg_cube::random_move_scramble(a.size, rg_cube::scramble_length(a.size), a.seed);
            println!("{}", rg_cube::format_alg(a.size, &moves));
            cube.apply_moves(&moves);
            print_net(&cube);
        }
        Some("solve") => {
            cube.apply_alg(a.positional.get(1).ok_or(USAGE)?)
                .map_err(|e| e.to_string())?;
            let mut solver = Solver::new();
            let t = now();
            solver.prepare(a.size).map_err(|e| e.to_string())?;
            println!("tables built in {:.0} ms", now() - t);
            let s = solver
                .solve(a.size, cube.facelets(), &a.opts, &now)
                .map_err(|e| e.to_string())?;
            println!("{}", rg_cube::format_alg(a.size, &s.moves));
            println!(
                "{} moves · {} · phases {:?} · {} nodes · {:.1} ms",
                s.moves.len(),
                s.algorithm,
                s.phase_lengths,
                s.nodes,
                s.search_ms
            );
        }
        Some("bench") => {
            let mut solver = Solver::new();
            let t = now();
            solver.prepare(a.size).map_err(|e| e.to_string())?;
            println!("tables built in {:.0} ms", now() - t);
            let (mut lengths, mut times) = (Vec::new(), Vec::new());
            for seed in 0..a.count {
                let mut c = rg_cube::Cube::new(a.size).map_err(|e| e.to_string())?;
                c.apply_moves(&rg_cube::random_move_scramble(
                    a.size,
                    40,
                    seed ^ a.seed << 32,
                ));
                let s = solver
                    .solve(a.size, c.facelets(), &a.opts, &now)
                    .map_err(|e| e.to_string())?;
                lengths.push(s.moves.len());
                times.push(s.search_ms);
            }
            lengths.sort_unstable();
            times.sort_by(f64::total_cmp);
            let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len() as f64;
            let lf: Vec<f64> = lengths.iter().map(|&l| l as f64).collect();
            let pct = |v: &[f64], p: f64| v[((v.len() - 1) as f64 * p) as usize];
            println!(
                "{} solves · length mean {:.2}, max {} · time mean {:.1} ms, p95 {:.1} ms, max {:.1} ms",
                a.count,
                mean(&lf),
                lengths.last().copied().unwrap_or(0),
                mean(&times),
                pct(&times, 0.95),
                times.last().copied().unwrap_or(0.0)
            );
        }
        _ => return Err(USAGE.into()),
    }
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}
