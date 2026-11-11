//! Benchmarks for search algorithms
//!
//! Measures performance of equation search with varying complexity levels
//! and parallel vs sequential execution.

#![allow(clippy::field_reassign_with_default)]

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use ries_rs::gen::GenConfig;
use ries_rs::search::{search, search_with_stats, ExprDatabase, SearchConfig};

fn bench_search_levels(c: &mut Criterion) {
    let mut group = c.benchmark_group("search_levels");

    // Level 1: minimal complexity
    let mut config_l1 = GenConfig::default();
    config_l1.max_lhs_complexity = 20;
    config_l1.max_rhs_complexity = 15;

    // Level 2: standard complexity
    let mut config_l2 = GenConfig::default();
    config_l2.max_lhs_complexity = 43;
    config_l2.max_rhs_complexity = 36;

    // Level 3: higher complexity
    let mut config_l3 = GenConfig::default();
    config_l3.max_lhs_complexity = 60;
    config_l3.max_rhs_complexity = 50;

    let target = 2.5;
    let max_matches = 50;

    group.bench_with_input(BenchmarkId::new("level", 1), &config_l1, |b, config| {
        b.iter(|| search(black_box(target), black_box(config), black_box(max_matches)))
    });

    group.bench_with_input(BenchmarkId::new("level", 2), &config_l2, |b, config| {
        b.iter(|| search(black_box(target), black_box(config), black_box(max_matches)))
    });

    group.bench_with_input(BenchmarkId::new("level", 3), &config_l3, |b, config| {
        b.iter(|| search(black_box(target), black_box(config), black_box(max_matches)))
    });

    group.finish();
}

fn bench_different_targets(c: &mut Criterion) {
    let mut group = c.benchmark_group("search_targets");

    let mut config = GenConfig::default();
    config.max_lhs_complexity = 40;
    config.max_rhs_complexity = 35;

    let targets = [
        ("pi", std::f64::consts::PI),
        ("e", std::f64::consts::E),
        ("sqrt2", std::f64::consts::SQRT_2),
        ("golden", std::f64::consts::GOLDEN_RATIO),
        ("arbitrary", 2.345_678),
    ];

    let max_matches = 50;

    for (name, target) in targets {
        group.bench_with_input(BenchmarkId::new("target", name), &target, |b, &target| {
            b.iter(|| {
                search(
                    black_box(target),
                    black_box(&config),
                    black_box(max_matches),
                )
            })
        });
    }

    group.finish();
}

#[cfg(feature = "parallel")]
fn bench_parallel_vs_sequential(c: &mut Criterion) {
    use ries_rs::search::search_parallel;

    let mut group = c.benchmark_group("parallel_comparison");

    let mut config = GenConfig::default();
    config.max_lhs_complexity = 50;
    config.max_rhs_complexity = 45;

    let target = 2.5;
    let max_matches = 50;

    group.bench_function("sequential", |b| {
        b.iter(|| {
            search(
                black_box(target),
                black_box(&config),
                black_box(max_matches),
            )
        })
    });

    group.bench_function("parallel", |b| {
        b.iter(|| {
            search_parallel(
                black_box(target),
                black_box(&config),
                black_box(max_matches),
            )
        })
    });

    group.finish();
}

fn bench_match_finding(c: &mut Criterion) {
    // Benchmark just the matching portion (after generation)
    use ries_rs::gen::generate_all;

    let mut group = c.benchmark_group("match_finding");

    let mut config = GenConfig::default();
    config.max_lhs_complexity = 50;
    config.max_rhs_complexity = 45;

    let target = 2.5;

    // Pre-generate expressions
    let generated = generate_all(&config, target);

    group.bench_function("database_insert", |b| {
        b.iter_batched(
            || generated.rhs.clone(),
            |rhs| {
                let mut db = ExprDatabase::new();
                db.insert_rhs(rhs);
                db
            },
            criterion::BatchSize::SmallInput,
        )
    });

    let mut db = ExprDatabase::new();
    db.insert_rhs(generated.rhs.clone());

    let search_config = SearchConfig {
        target,
        max_matches: 100,
        ..Default::default()
    };

    group.bench_function("find_matches", |b| {
        b.iter(|| db.find_matches(black_box(&generated.lhs), black_box(&search_config)))
    });

    group.finish();
}

fn bench_statistics_collection(c: &mut Criterion) {
    let mut group = c.benchmark_group("statistics");

    let mut config = GenConfig::default();
    config.max_lhs_complexity = 40;
    config.max_rhs_complexity = 35;

    let target = 2.5;
    let max_matches = 50;

    group.bench_function("without_stats", |b| {
        b.iter(|| {
            search(
                black_box(target),
                black_box(&config),
                black_box(max_matches),
            )
        })
    });

    group.bench_function("with_stats", |b| {
        b.iter(|| {
            search_with_stats(
                black_box(target),
                black_box(&config),
                black_box(max_matches),
            )
        })
    });

    group.finish();
}

// Keep stop-policy performance visible without weakening its correctness proof.
fn bench_stop_contract(c: &mut Criterion) {
    use ries_rs::pool::RankingMode;
    use ries_rs::search::search_with_stats_and_config;
    let mut group = c.benchmark_group("stop_contract");
    group.sample_size(10);
    let gen = GenConfig {
        max_lhs_complexity: 46,
        max_rhs_complexity: 46,
        ..Default::default()
    };
    for (name, target) in [
        ("pi", std::f64::consts::PI),
        ("half", 0.5),
        ("two_point_five", 2.5),
    ] {
        for (mode, stop_below) in [("exact", None), ("classic", Some(1e-10))] {
            let config = SearchConfig {
                target,
                max_matches: 16,
                stop_at_exact: true,
                stop_below,
                ranking_mode: RankingMode::Parity,
                ..Default::default()
            };
            group.bench_function(format!("{mode}/{name}"), |b| {
                b.iter(|| search_with_stats_and_config(black_box(&gen), black_box(&config)))
            });
        }
    }
    group.finish();
}

#[cfg(feature = "parallel")]
criterion_group!(
    benches,
    bench_search_levels,
    bench_different_targets,
    bench_parallel_vs_sequential,
    bench_match_finding,
    bench_statistics_collection,
    bench_stop_contract,
);

#[cfg(not(feature = "parallel"))]
criterion_group!(
    benches,
    bench_search_levels,
    bench_different_targets,
    bench_match_finding,
    bench_statistics_collection,
    bench_stop_contract,
);

criterion_main!(benches);
