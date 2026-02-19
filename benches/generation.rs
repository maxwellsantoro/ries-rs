//! Benchmarks for expression generation
//!
//! Measures performance of LHS and RHS expression generation at various
//! complexity levels.

#![allow(clippy::field_reassign_with_default)]

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use ries_rs::gen::{generate_all, GenConfig};
#[cfg(feature = "parallel")]
use ries_rs::gen::generate_all_parallel;

fn bench_lhs_generation(c: &mut Criterion) {
    let mut group = c.benchmark_group("lhs_generation");

    let target = 2.5;

    // Vary LHS complexity while keeping RHS fixed
    let complexities = [
        ("tiny", 15, 15),
        ("small", 25, 20),
        ("medium", 40, 35),
        ("large", 60, 50),
        ("xlarge", 80, 65),
    ];

    for (name, lhs_comp, rhs_comp) in complexities {
        let mut config = GenConfig::default();
        config.max_lhs_complexity = lhs_comp;
        config.max_rhs_complexity = rhs_comp;

        group.bench_with_input(BenchmarkId::new("lhs", name), &config, |b, config| {
            b.iter(|| {
                let result = generate_all(black_box(config), black_box(target));
                black_box(result.lhs.len())
            })
        });
    }

    group.finish();
}

fn bench_rhs_generation(c: &mut Criterion) {
    let mut group = c.benchmark_group("rhs_generation");

    let target = 2.5;

    // Vary RHS complexity while keeping LHS fixed
    let complexities = [
        ("tiny", 15, 15),
        ("small", 25, 20),
        ("medium", 40, 35),
        ("large", 60, 50),
    ];

    for (name, lhs_comp, rhs_comp) in complexities {
        let mut config = GenConfig::default();
        config.max_lhs_complexity = lhs_comp;
        config.max_rhs_complexity = rhs_comp;

        group.bench_with_input(BenchmarkId::new("rhs", name), &config, |b, config| {
            b.iter(|| {
                let result = generate_all(black_box(config), black_box(target));
                black_box(result.rhs.len())
            })
        });
    }

    group.finish();
}

fn bench_full_generation(c: &mut Criterion) {
    let mut group = c.benchmark_group("full_generation");

    let target = 2.5;

    let configs = [("level1", 20, 15), ("level2", 43, 36), ("level3", 60, 50)];

    for (name, lhs_comp, rhs_comp) in configs {
        let mut config = GenConfig::default();
        config.max_lhs_complexity = lhs_comp;
        config.max_rhs_complexity = rhs_comp;

        group.bench_with_input(BenchmarkId::new("full", name), &config, |b, config| {
            b.iter(|| {
                let result = generate_all(black_box(config), black_box(target));
                black_box((result.lhs.len(), result.rhs.len()))
            })
        });
    }

    group.finish();
}

#[cfg(feature = "parallel")]
fn bench_parallel_generation(c: &mut Criterion) {
    let mut group = c.benchmark_group("parallel_generation");

    let target = 2.5;

    let mut config = GenConfig::default();
    config.max_lhs_complexity = 60;
    config.max_rhs_complexity = 50;

    group.bench_function("sequential", |b| {
        b.iter(|| {
            let result = generate_all(black_box(&config), black_box(target));
            black_box((result.lhs.len(), result.rhs.len()))
        })
    });

    group.bench_function("parallel", |b| {
        b.iter(|| {
            let result = generate_all_parallel(black_box(&config), black_box(target));
            black_box((result.lhs.len(), result.rhs.len()))
        })
    });

    group.finish();
}

fn bench_expression_count(c: &mut Criterion) {
    let mut group = c.benchmark_group("expression_counts");

    let target = 2.5;

    // Measure how many expressions are generated at various levels
    let configs = [
        ("level1", 20, 15),
        ("level2", 43, 36),
        ("level3", 60, 50),
        ("level4", 80, 65),
    ];

    for (name, lhs_comp, rhs_comp) in configs {
        let mut config = GenConfig::default();
        config.max_lhs_complexity = lhs_comp;
        config.max_rhs_complexity = rhs_comp;

        let result = generate_all(&config, target);

        group.bench_with_input(
            BenchmarkId::new("count", name),
            &(result.lhs.len(), result.rhs.len()),
            |b, counts| b.iter(|| black_box(counts)),
        );
    }

    group.finish();
}

#[cfg(feature = "parallel")]
criterion_group!(
    benches,
    bench_lhs_generation,
    bench_rhs_generation,
    bench_full_generation,
    bench_parallel_generation,
    bench_expression_count,
);

#[cfg(not(feature = "parallel"))]
criterion_group!(
    benches,
    bench_lhs_generation,
    bench_rhs_generation,
    bench_full_generation,
    bench_expression_count,
);

criterion_main!(benches);
