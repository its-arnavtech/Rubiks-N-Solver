//! M3.5: the committed `artifacts/macros/library.json` is valid, deterministic, and its actions
//! act on real cubes exactly as declared (checked through color extraction, an independent
//! code path from the labeled verifier).

use std::path::PathBuf;

use nx_macro::{Binding, Library, MACRO_KINDS, TypeLibrary, orientation_mod, sym};
use nx_sim::rng::{below, between, rng};
use nx_sim::{SlotMap, orbits, random_state};

fn committed() -> Library {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../")
        .join(nx_macro::LIBRARY_PATH);
    Library::load(&path).expect("committed library loads and its sha256 matches")
}

#[test]
fn regenerating_gives_the_committed_library() {
    let lib = committed();
    let (fresh, _) = nx_macro::library::build("test");
    assert_eq!(
        fresh.sha256, lib.sha256,
        "run `just discover` and commit the result"
    );
    assert_eq!(fresh.types, lib.types);
}

#[test]
fn every_type_is_present_with_its_type_id() {
    let lib = committed();
    for kind in MACRO_KINDS {
        let t = lib.get(kind).unwrap_or_else(|| panic!("{kind} missing"));
        assert_eq!(Some(t.type_id), kind.type_id());
        assert_eq!(t.slots, kind.slot_count());
        assert!(!t.actions.is_empty());
    }
}

#[test]
fn orbit_space_matches_real_cube_on_random_states() {
    let lib = committed();
    let mut r = rng(4242);
    for case in 0..40 {
        let n = between(&mut r, 4, 30) as u32;
        let mut cube = random_state(n, case);
        let os = orbits(n);
        for kind in MACRO_KINDS {
            let candidates: Vec<_> = os.iter().filter(|o| o.kind == kind).collect();
            if candidates.is_empty() {
                continue;
            }
            let orbit = candidates[below(&mut r, candidates.len() as u64) as usize];
            let t: &TypeLibrary = lib.get(kind).unwrap();
            let map = SlotMap::for_orbit(n, orbit);
            let b = Binding::for_orbit(n, orbit);
            let m = orientation_mod(kind);
            let mut content = map.extract(&cube).unwrap();
            for _ in 0..5 {
                let a = &t.actions[below(&mut r, t.actions.len() as u64) as usize];
                content = TypeLibrary::effect(a).apply(&content, m);
                cube.apply_all(&sym::instantiate(&t.action_moves(a).unwrap(), b));
                assert_eq!(
                    map.extract(&cube).unwrap(),
                    content,
                    "n={n} {kind} action {}",
                    a.id
                );
            }
        }
        assert_eq!(nx_sim::validate(&cube), Ok(()));
    }
}
