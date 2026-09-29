//! Graph-theoretic cube solvers (docs/07, docs/08).
//!
//! * 2x2: exact distances over the whole 3,674,160-vertex graph, greedy descent (optimal).
//! * 3x3: Kociemba two-phase: IDA* over two coset graphs with quotient-graph heuristics.
//! * 4x4 and 5x5: reduction chains of graph searches over quotient graphs, then the 3x3.
//!
//! Every solution is checked on the facelet model before it is returned.

pub mod cubie;
pub mod kociemba;
pub mod macros5;
pub mod nxn;
pub mod nxn5;
pub mod pocket;
pub mod slots;

use std::fmt;

use cubie::{CubieCube, DBL, InvalidCube, corner_facelets, move_cubies};
use kociemba::Kociemba;
use nxn::Reduction4;
use nxn5::Reduction5;
use pocket::Pocket;
use rg_cube::{Cube, Geometry, Move, conjugate, whole_cube_rotations};

#[derive(Clone, Debug)]
pub struct SolveOptions {
    /// Stop as soon as a solution this short is found (3x3).
    pub target_length: usize,
    /// Search budget in milliseconds; honoured once a first solution exists (3x3).
    pub max_time_ms: f64,
}

impl Default for SolveOptions {
    fn default() -> Self {
        Self {
            target_length: 20,
            max_time_ms: 1500.0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Solution {
    /// Moves in the caller's frame (no whole-cube rotations).
    pub moves: Vec<Move>,
    pub algorithm: &'static str,
    pub optimal: bool,
    pub phase_lengths: Vec<usize>,
    pub nodes: u64,
    pub search_ms: f64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SolveError {
    Unsupported(u8),
    Invalid(InvalidCube),
    /// A search phase ran out of its time budget.
    Timeout(&'static str),
    /// A solution failed verification: always a bug.
    Internal(&'static str),
}

impl fmt::Display for SolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported(n) => write!(
                f,
                "no solver for the {n}x{n}: this project covers the 2x2 to the 5x5"
            ),
            Self::Invalid(e) => write!(f, "this cube cannot be solved: {e}"),
            Self::Timeout(phase) => write!(f, "{phase} ran out of time"),
            Self::Internal(e) => write!(f, "internal error: {e}"),
        }
    }
}

impl std::error::Error for SolveError {}

impl From<InvalidCube> for SolveError {
    fn from(e: InvalidCube) -> Self {
        Self::Invalid(e)
    }
}

/// Holds lazily built lookup tables for each supported size.
#[derive(Default)]
pub struct Solver {
    pocket: Option<Pocket>,
    kociemba: Option<(Kociemba, Vec<Move>)>,
    reduction4: Option<Reduction4>,
    reduction5: Option<Reduction5>,
}

/// Time allowed for each big-cube reduction phase; generous, since a failure is final.
const REDUCTION_PHASE_BUDGET_MS: f64 = 30_000.0;

impl Solver {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn supports(n: u8) -> bool {
        matches!(n, 2..=5)
    }

    /// Build the tables for size `n` if needed. Returns true if anything was built.
    pub fn prepare(&mut self, n: u8) -> Result<bool, SolveError> {
        match n {
            2 if self.pocket.is_none() => {
                self.pocket = Some(Pocket::new());
                Ok(true)
            }
            3 if self.kociemba.is_none() => {
                let moves =
                    rg_cube::parse_alg(3, "U U2 U' R R2 R' F F2 F' D D2 D' L L2 L' B B2 B'")
                        .expect("valid notation");
                self.kociemba = Some((Kociemba::new(move_cubies(&moves)), moves));
                Ok(true)
            }
            // The 4x4 finishes as a 3x3, so it needs the 3x3 tables as well.
            4 if self.reduction4.is_none() || self.kociemba.is_none() => {
                self.prepare(3)?;
                self.reduction4.get_or_insert_with(Reduction4::new);
                Ok(true)
            }
            5 if self.reduction5.is_none() || self.kociemba.is_none() => {
                self.prepare(3)?;
                self.reduction5.get_or_insert_with(Reduction5::new);
                Ok(true)
            }
            2..=5 => Ok(false),
            _ => Err(SolveError::Unsupported(n)),
        }
    }

    /// Solve the cube shown by `facelets` (colour = home face index, U R F D L B order).
    /// `now_ms` is a monotonic clock, supplied by the caller so this works natively and in wasm.
    pub fn solve(
        &mut self,
        n: u8,
        facelets: &[u8],
        opts: &SolveOptions,
        now_ms: &dyn Fn() -> f64,
    ) -> Result<Solution, SolveError> {
        self.prepare(n)?;
        let per_face = usize::from(n).pow(2);
        if facelets.len() != 6 * per_face {
            return Err(InvalidCube::WrongLength.into());
        }
        if (0..6).any(|c| facelets.iter().filter(|&&x| x == c).count() != per_face) {
            return Err(InvalidCube::ColourCount.into());
        }
        let geom = Geometry::new(n);
        let start = now_ms();

        // Normalize the frame: fixed centres (3x3) or the DBL corner home (2x2).
        let (rotation, normalized) = match n {
            2 => {
                let dbl = corner_facelets(&geom)[DBL];
                normalize(n, facelets, |f| {
                    dbl.iter().zip([3, 5, 4]).all(|(&i, c)| f[i] == c)
                })
                .ok_or(InvalidCube::NoReferenceCorner)?
            }
            3 => normalize(n, facelets, |f| {
                (0..6).all(|face| f[face * 9 + 4] == face as u8)
            })
            .ok_or(InvalidCube::CentresMisplaced)?,
            // Big cubes: the phases choose the orientation themselves (docs/08 §5.1).
            _ => (Vec::new(), facelets.to_vec()),
        };

        let (frame_moves, algorithm, optimal, phase_lengths, nodes) = match n {
            2 => {
                let pocket = self.pocket.as_ref().expect("prepared");
                let moves = pocket.solve(&geom, &normalized)?;
                let len = moves.len();
                (
                    moves,
                    "Exact BFS distance table (whole 2x2 graph)",
                    true,
                    vec![len],
                    0,
                )
            }
            4 => self.solve_4x4(&normalized, opts, now_ms)?,
            5 => self.solve_5x5(&normalized, opts, now_ms)?,
            _ => {
                let (koc, ids) = self.kociemba.as_ref().expect("prepared");
                let cube = CubieCube::from_facelets(&normalized)?;
                let deadline = start + opts.max_time_ms;
                let r = koc.solve(&cube, opts.target_length, &|| now_ms() > deadline);
                let moves: Vec<Move> = r.moves.iter().map(|&m| ids[usize::from(m)]).collect();
                let lengths = vec![r.phase1_len, r.moves.len() - r.phase1_len];
                (
                    moves,
                    "Kociemba two-phase (IDA* on coset graphs)",
                    false,
                    lengths,
                    r.nodes,
                )
            }
        };

        // Map back to the caller's frame, merge turns across phase boundaries, and verify
        // on the facelet model.
        let moves: Vec<Move> = frame_moves
            .iter()
            .map(|&m| conjugate(&geom, &rotation, m))
            .collect();
        let moves = rg_cube::simplify(&moves);
        let mut check = Cube::new(n).map_err(|_| SolveError::Internal("cube size"))?;
        check
            .set_facelets(facelets)
            .map_err(|_| SolveError::Internal("facelets"))?;
        check.apply_moves(&moves);
        if !check.is_solved() {
            return Err(SolveError::Internal("solution failed verification"));
        }
        Ok(Solution {
            moves,
            algorithm,
            optimal,
            phase_lengths,
            nodes,
            search_ms: now_ms() - start,
        })
    }
}

type Found = (Vec<Move>, &'static str, bool, Vec<usize>, u64);

impl Solver {
    /// 4x4: three reduction phases (graph searches over quotient graphs), then the
    /// reduced cube is finished as a 3x3 by Kociemba (docs/08 §5).
    fn solve_4x4(
        &self,
        facelets: &[u8],
        opts: &SolveOptions,
        now_ms: &dyn Fn() -> f64,
    ) -> Result<Found, SolveError> {
        let red = self.reduction4.as_ref().expect("prepared");
        let (koc, ids) = self.kociemba.as_ref().expect("prepared");
        let geom = Geometry::new(4);
        cubie::corners_from_facelets(&geom, facelets)?;
        if red.slots.wings[0].pieces(facelets).is_none() {
            return Err(InvalidCube::UnknownEdge.into());
        }
        let mut cube = Cube::new(4).map_err(|_| SolveError::Internal("cube size"))?;
        cube.set_facelets(facelets)
            .map_err(|_| SolveError::Internal("facelets"))?;
        let mut moves = Vec::new();
        let mut lengths = Vec::new();
        let mut take = |cube: &mut Cube, path: Vec<Move>, lengths: &mut Vec<usize>| {
            lengths.push(path.len());
            cube.apply_moves(&path);
            moves.extend(path);
        };
        let to_moves = |path: Vec<usize>| path.into_iter().map(|m| red.moves[m]).collect();

        let (p1, pair) = red.phase1(cube.facelets());
        take(&mut cube, to_moves(p1), &mut lengths);

        let deadline = now_ms() + REDUCTION_PHASE_BUDGET_MS;
        let p2 = red
            .phase2(cube.facelets(), pair, &|| now_ms() > deadline)
            .ok_or(SolveError::Timeout("4x4 phase 2"))?;
        take(&mut cube, to_moves(p2), &mut lengths);

        let deadline = now_ms() + REDUCTION_PHASE_BUDGET_MS;
        let p3 = red
            .phase3(cube.facelets(), &|| now_ms() > deadline)
            .ok_or(SolveError::Timeout("4x4 phase 3"))?;
        take(&mut cube, to_moves(p3), &mut lengths);

        let three = red
            .reduce_to_3x3(cube.facelets())
            .ok_or(SolveError::Internal("reduction left the cube unreduced"))?;
        let cube3 = CubieCube::from_facelets(&three)
            .map_err(|_| SolveError::Internal("reduction left a parity"))?;
        let deadline = now_ms() + opts.max_time_ms;
        let r = koc.solve(&cube3, opts.target_length, &|| now_ms() > deadline);
        let finish: Vec<Move> = r
            .moves
            .iter()
            .map(|&m| Reduction4::lift_3x3_move(ids[usize::from(m)]))
            .collect::<Option<_>>()
            .ok_or(SolveError::Internal("3x3 move with no 4x4 equivalent"))?;
        take(&mut cube, finish, &mut lengths);
        Ok((
            moves,
            "4x4 reduction (3 graph phases) + Kociemba",
            false,
            lengths,
            r.nodes,
        ))
    }
}

impl Solver {
    /// 5x5: centres onto axes, edge orientation and parity, then centres and tredge pairing
    /// in stages; the reduced cube is finished as a 3x3 by Kociemba (docs/08 §6).
    fn solve_5x5(
        &self,
        facelets: &[u8],
        opts: &SolveOptions,
        now_ms: &dyn Fn() -> f64,
    ) -> Result<Found, SolveError> {
        let red = self.reduction5.as_ref().expect("prepared");
        let (koc, ids) = self.kociemba.as_ref().expect("prepared");
        cubie::corners_from_facelets(&Geometry::new(5), facelets)?;
        let mut cube = Cube::new(5).map_err(|_| SolveError::Internal("cube size"))?;
        cube.set_facelets(facelets)
            .map_err(|_| SolveError::Internal("facelets"))?;
        let mut moves = Vec::new();
        let mut lengths = Vec::new();
        type Phase = fn(&Reduction5, &[u8], &dyn Fn() -> bool) -> Option<Vec<usize>>;
        let phases: [(Option<Phase>, &'static str); 4] = [
            (Some(Reduction5::phase1), "5x5 phase 1"),
            (Some(Reduction5::phase2a), "5x5 phase 2a"),
            (Some(Reduction5::phase2b), "5x5 phase 2b"),
            (None, "5x5 phase 3"),
        ];
        for (phase, name) in phases {
            let deadline = now_ms() + REDUCTION_PHASE_BUDGET_MS;
            let path = match phase {
                Some(phase) => phase(red, cube.facelets(), &|| now_ms() > deadline),
                None => red.phase3(cube.facelets(), now_ms, deadline),
            }
            .ok_or(SolveError::Timeout(name))?;
            let path: Vec<Move> = path.into_iter().map(|m| red.moves[m]).collect();
            let path = nxn5::expand_obtm(&path);
            lengths.push(path.len());
            cube.apply_moves(&path);
            moves.extend(path);
        }
        let three = Reduction5::reduce_to_3x3(cube.facelets());
        let cube3 = CubieCube::from_facelets(&three)
            .map_err(|_| SolveError::Internal("5x5 reduction left an illegal 3x3"))?;
        let deadline = now_ms() + opts.max_time_ms;
        let r = koc.solve(&cube3, opts.target_length, &|| now_ms() > deadline);
        let finish: Vec<Move> = r
            .moves
            .iter()
            .map(|&m| Reduction5::lift_3x3_move(ids[usize::from(m)]))
            .collect::<Option<_>>()
            .ok_or(SolveError::Internal("3x3 move with no 5x5 equivalent"))?;
        lengths.push(finish.len());
        moves.extend(finish);
        Ok((
            moves,
            "5x5 reduction (4 graph phases) + Kociemba",
            false,
            lengths,
            r.nodes,
        ))
    }
}

/// Find a whole-cube rotation after which `ok` holds; returns it with the rotated facelets.
fn normalize(n: u8, facelets: &[u8], ok: impl Fn(&[u8]) -> bool) -> Option<(Vec<Move>, Vec<u8>)> {
    let mut cube = Cube::new(n).ok()?;
    whole_cube_rotations(n).into_iter().find_map(|rot| {
        cube.set_facelets(facelets).ok()?;
        cube.apply_moves(&rot);
        ok(cube.facelets()).then(|| (rot, cube.facelets().to_vec()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clock() -> impl Fn() -> f64 {
        let t0 = std::time::Instant::now();
        move || t0.elapsed().as_secs_f64() * 1000.0
    }

    #[test]
    fn solves_2x2_optimally_in_any_orientation() {
        let mut solver = Solver::new();
        let now = clock();
        for seed in 0..30 {
            let mut c = Cube::new(2).unwrap();
            c.apply_moves(&rg_cube::random_move_scramble(2, 14, seed));
            c.apply_alg("x y' D L2 B").unwrap(); // moves the DBL corner away
            let s = solver
                .solve(2, c.facelets(), &SolveOptions::default(), &now)
                .unwrap();
            assert!(s.moves.len() <= 11, "2x2 God's number is 11");
            c.apply_moves(&s.moves);
            assert!(c.is_solved());
        }
    }

    #[test]
    fn solves_3x3_from_any_orientation() {
        let mut solver = Solver::new();
        let now = clock();
        let opts = SolveOptions {
            target_length: 22,
            max_time_ms: 2000.0,
        };
        for seed in 0..12 {
            let mut c = Cube::new(3).unwrap();
            c.apply_moves(&rg_cube::random_move_scramble(3, 30, seed));
            c.apply_alg("x2 M E' Rw y").unwrap(); // moves the centres too
            let s = solver.solve(3, c.facelets(), &opts, &now).unwrap();
            assert!(s.moves.len() <= 30, "got {} moves", s.moves.len());
            c.apply_moves(&s.moves);
            assert!(c.is_solved());
        }
    }

    #[test]
    fn solves_4x4_end_to_end() {
        let mut solver = Solver::new();
        let now = clock();
        let mut lengths = Vec::new();
        for seed in 0..12 {
            let mut c = Cube::new(4).unwrap();
            c.apply_moves(&rg_cube::random_move_scramble(4, 40, 900 + seed));
            let s = solver
                .solve(4, c.facelets(), &SolveOptions::default(), &now)
                .unwrap();
            c.apply_moves(&s.moves);
            assert!(c.is_solved(), "seed {seed}");
            lengths.push(s.moves.len());
            println!(
                "seed {seed}: {} moves, phases {:?}, {:.0} ms",
                s.moves.len(),
                s.phase_lengths,
                s.search_ms
            );
        }
        println!(
            "4x4 mean length {:.1}",
            lengths.iter().sum::<usize>() as f64 / lengths.len() as f64
        );
    }

    #[test]
    fn solves_5x5_end_to_end() {
        let mut solver = Solver::new();
        let now = clock();
        for seed in 0..12 {
            let mut c = Cube::new(5).unwrap();
            c.apply_moves(&rg_cube::random_move_scramble(5, 60, 700 + seed));
            let s = solver
                .solve(5, c.facelets(), &SolveOptions::default(), &now)
                .unwrap();
            c.apply_moves(&s.moves);
            assert!(c.is_solved(), "seed {seed}");
            println!(
                "seed {seed}: {} moves, phases {:?}, {:.0} ms",
                s.moves.len(),
                s.phase_lengths,
                s.search_ms
            );
        }
    }

    #[test]
    fn rejects_illegal_and_unsupported_cubes() {
        let mut solver = Solver::new();
        let now = clock();
        let mut f: Vec<u8> = (0..54).map(|i| (i / 9) as u8).collect();
        f.swap(5, 10); // flip one edge
        let e = solver
            .solve(3, &f, &SolveOptions::default(), &now)
            .unwrap_err();
        assert_eq!(e, SolveError::Invalid(InvalidCube::EdgeFlip));
        let six = vec![0u8; 216];
        assert_eq!(
            solver
                .solve(6, &six, &SolveOptions::default(), &now)
                .unwrap_err(),
            SolveError::Unsupported(6)
        );
    }
}
