//! M1.5 performance budgets (ARCHITECTURE §14), run with `just bench`:
//! inner slice move at N=400 ≤ 10 µs, face move at N=400 ≤ 1 ms, 1M-move replay at N=100 ≤ 2 s.

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use nx_sim::{Axis, Cube, LabeledCube, Move, scramble_moves};

fn moves(c: &mut Criterion) {
    let n = 400;
    let mut cube = Cube::scramble(n, 50, 1).0;
    let inner = Move::new(Axis::X, n / 2, 1);
    c.bench_function("inner_move_n400", |b| {
        b.iter(|| cube.apply(black_box(inner)))
    });
    let face = Move::new(Axis::Y, 0, 1);
    c.bench_function("face_move_n400", |b| b.iter(|| cube.apply(black_box(face))));
    let mut labeled = LabeledCube::solved(n);
    c.bench_function("inner_move_n400_labeled", |b| {
        b.iter(|| labeled.apply(black_box(inner)))
    });
}

fn replay(c: &mut Criterion) {
    let n = 100;
    let seq = scramble_moves(n, 1_000_000, 0);
    let mut g = c.benchmark_group("replay");
    g.sample_size(10)
        .measurement_time(std::time::Duration::from_secs(10));
    g.bench_function("replay_1m_n100", |b| {
        b.iter(|| {
            let mut cube = Cube::solved(n);
            cube.apply_all(black_box(&seq));
            cube
        })
    });
    g.finish();
}

criterion_group!(benches, moves, replay);
criterion_main!(benches);
