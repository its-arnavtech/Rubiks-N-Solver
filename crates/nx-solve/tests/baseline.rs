//! M4: verified baseline solves end to end, and orbit space ≡ real cube (M4.5).

use std::path::PathBuf;
use std::sync::OnceLock;

use nx_macro::{Binding, Library, sym};
use nx_sim::rng::{below, between, rng};
use nx_sim::{Cube, OrbitKind, SlotMap, orbits, random_state};
use nx_solve::{SolveError, SolverLib, solve_baseline};

fn lib() -> &'static SolverLib {
    static LIB: OnceLock<SolverLib> = OnceLock::new();
    LIB.get_or_init(|| {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../")
            .join(nx_macro::LIBRARY_PATH);
        SolverLib::new(&Library::load(&path).unwrap()).unwrap()
    })
}

#[test]
fn random_states_solve_and_verify() {
    for n in 2..=14 {
        for seed in 0..12 {
            let cube = random_state(n, seed);
            let out =
                solve_baseline(&cube, lib()).unwrap_or_else(|e| panic!("n={n} seed={seed}: {e}"));
            assert!(out.result.verified);
            assert_eq!(out.result.raw_len as usize, out.raw.len());
            assert!(out.cancelled.len() <= out.raw.len());
            let mut c = cube.clone();
            c.apply_all(&out.cancelled);
            assert!(c.is_solved());
            // Segments tile the raw list.
            let mut at = 0;
            for s in &out.result.segments {
                assert_eq!(s.start, at);
                at = s.end;
            }
            assert_eq!(at, out.result.raw_len);
        }
    }
}

#[test]
fn scrambles_and_solved_cubes() {
    for n in [2, 3, 4, 5, 17, 24] {
        let (cube, _) = Cube::scramble(n, 300, 3);
        assert!(solve_baseline(&cube, lib()).unwrap().result.verified);
        let out = solve_baseline(&Cube::solved(n), lib()).unwrap();
        assert_eq!(out.result.raw_len, 0);
    }
}

#[test]
fn invalid_input_is_rejected() {
    let mut cube = random_state(5, 1);
    cube.set_stickers(&[(0, 1), (100, 0)]);
    assert!(matches!(
        solve_baseline(&cube, lib()),
        Err(SolveError::Invalid(_))
    ));
}

#[test]
fn result_json_round_trips() {
    let out = solve_baseline(&random_state(6, 9), lib()).unwrap();
    let text = serde_json::to_string(&out.result).unwrap();
    let mut back: nx_solve::SolveResult = serde_json::from_str(&text).unwrap();
    // Float timings need not round-trip bit-exactly; everything else must.
    let mut want = out.result.clone();
    back.timings_ms = Default::default();
    want.timings_ms = Default::default();
    assert_eq!(back, want);
    assert!(text.contains("\"phase\":\"orbits\""));
}

/// M4.5: applying an action in orbit space equals applying its moves and re-extracting.
#[test]
fn orbit_space_equals_real_cube() {
    let mut r = rng(77);
    for case in 0..30 {
        let n = between(&mut r, 4, 30) as u32;
        let mut cube = random_state(n, 1000 + case);
        let os = orbits(n);
        let o = &os[below(&mut r, os.len() as u64) as usize];
        if o.kind == OrbitKind::FixedCenter {
            continue;
        }
        let kl = lib().kind(o.kind);
        let map = SlotMap::for_orbit(n, o);
        let mut content = map.extract(&cube).unwrap();
        for _ in 0..8 {
            let a = &kl.actions[below(&mut r, kl.actions.len() as u64) as usize];
            content = a.effect.apply(&content, kl.m);
            cube.apply_all(&sym::instantiate(&a.moves, Binding::for_orbit(n, o)));
            assert_eq!(map.extract(&cube).unwrap(), content, "n={n} {}", o.kind);
        }
    }
}
