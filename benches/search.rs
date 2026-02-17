//! Benchmarks for search algorithms
//!
//! Measures performance of equation search with varying complexity levels
//! and parallel vs sequential execution.

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
        ("golden", 1.618_033_988_749_895),
        ("arbitrary", 2.718_281_828),
    ];

    let max_matches = 50;

    for (name, target) in targets {
        group.bench_with_input(BenchmarkId::new("target", name), &target, |b, &target| {
            b.iter(|| search(black_box(target), black_box(&config), black_box(max_matches)))
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
        b.iter(|| search(black_box(target), black_box(&config), black_box(max_matches)))
    });

    group.bench_function("parallel", |b| {
        b.iter(|| search_parallel(black_box(target), black_box(&config), black_box(max_matches)))
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
        b.iter(|| search(black_box(target), black_box(&config), black_box(max_matches)))
    });

    group.bench_function("with_stats", |b| {
        b.iter(|| search_with_stats(black_box(target), black_box(&config), black_box(max_matches)))
    });

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
);

#[cfg(not(feature = "parallel"))]
criterion_group!(
    benches,
    bench_search_levels,
    bench_different_targets,
    bench_match_finding,
    bench_statistics_collection,
);

criterion_main!(benches);
