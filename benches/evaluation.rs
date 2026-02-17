//! Benchmarks for expression evaluation
//!
//! Measures performance of expression evaluation with various expression types
//! and workspace strategies.

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use ries_rs::eval::{evaluate, evaluate_fast, evaluate_with_workspace, EvalWorkspace};
use ries_rs::expr::Expression;

/// Create test expressions of varying complexity
fn simple_expr() -> Expression {
    Expression::parse("32+").unwrap() // 3 + 2
}

fn variable_expr() -> Expression {
    Expression::parse("x2*").unwrap() // x * 2
}

fn complex_expr() -> Expression {
    // x^2 + 2*x + 1
    Expression::parse("xs2x*+1+").unwrap()
}

fn trig_expr() -> Expression {
    // sin(πx) + cos(πx)
    Expression::parse("xSxC+").unwrap()
}

fn lambert_w_expr() -> Expression {
    // W(x) - Lambert W function
    Expression::parse("xW").unwrap()
}

fn x_to_x_expr() -> Expression {
    // x^x (classic RIES expression)
    Expression::parse("xx^").unwrap()
}

fn bench_simple_evaluation(c: &mut Criterion) {
    let expr = simple_expr();
    c.bench_function("eval_simple", |b| {
        b.iter(|| evaluate(black_box(&expr), black_box(2.5)))
    });
}

fn bench_variable_evaluation(c: &mut Criterion) {
    let expr = variable_expr();
    c.bench_function("eval_variable", |b| {
        b.iter(|| evaluate(black_box(&expr), black_box(2.5)))
    });
}

fn bench_complex_evaluation(c: &mut Criterion) {
    let expr = complex_expr();
    c.bench_function("eval_complex", |b| {
        b.iter(|| evaluate(black_box(&expr), black_box(2.5)))
    });
}

fn bench_trig_evaluation(c: &mut Criterion) {
    let expr = trig_expr();
    c.bench_function("eval_trig", |b| {
        b.iter(|| evaluate(black_box(&expr), black_box(2.5)))
    });
}

fn bench_lambert_w_evaluation(c: &mut Criterion) {
    let expr = lambert_w_expr();
    c.bench_function("eval_lambert_w", |b| {
        b.iter(|| evaluate(black_box(&expr), black_box(1.5)))
    });
}

fn bench_x_to_x_evaluation(c: &mut Criterion) {
    let expr = x_to_x_expr();
    c.bench_function("eval_x_to_x", |b| {
        b.iter(|| evaluate(black_box(&expr), black_box(2.5)))
    });
}

fn bench_workspace_vs_allocating(c: &mut Criterion) {
    let expr = complex_expr();
    let mut group = c.benchmark_group("workspace_comparison");

    group.bench_function("allocating", |b| {
        b.iter(|| evaluate(black_box(&expr), black_box(2.5)))
    });

    group.bench_function("reusable_workspace", |b| {
        let mut workspace = EvalWorkspace::new();
        b.iter(|| {
            evaluate_with_workspace(black_box(&expr), black_box(2.5), black_box(&mut workspace))
        })
    });

    group.bench_function("thread_local", |b| {
        b.iter(|| evaluate_fast(black_box(&expr), black_box(2.5)))
    });

    group.finish();
}

fn bench_multiple_x_values(c: &mut Criterion) {
    let expr = complex_expr();
    let x_values: Vec<f64> = (0..100).map(|i| (i as f64) * 0.1 - 5.0).collect();

    let mut group = c.benchmark_group("batch_evaluation");

    group.bench_function("batch_allocating", |b| {
        b.iter(|| {
            for &x in &x_values {
                let _ = evaluate(black_box(&expr), black_box(x));
            }
        })
    });

    group.bench_function("batch_workspace", |b| {
        let mut workspace = EvalWorkspace::new();
        b.iter(|| {
            for &x in &x_values {
                let _ = evaluate_with_workspace(
                    black_box(&expr),
                    black_box(x),
                    black_box(&mut workspace),
                );
            }
        })
    });

    group.bench_function("batch_thread_local", |b| {
        b.iter(|| {
            for &x in &x_values {
                let _ = evaluate_fast(black_box(&expr), black_box(x));
            }
        })
    });

    group.finish();
}

fn bench_expression_sizes(c: &mut Criterion) {
    let mut group = c.benchmark_group("expression_sizes");

    let expressions = [
        ("tiny", "x"),                    // just x
        ("small", "x2*"),                 // x * 2
        ("medium", "xs2x*+1+"),           // x^2 + 2x + 1
        ("large", "xs2x*+1+xs+"),         // x^2 + 2x + 1 + x^2
        ("xlarge", "xs2x*+1+xs+xs2x*++"), // x^2 + 2x + 1 + x^2 + x^2 + 2x
    ];

    for (name, postfix) in expressions {
        let expr = Expression::parse(postfix).unwrap();
        group.bench_with_input(BenchmarkId::new("eval", name), &expr, |b, expr| {
            b.iter(|| evaluate(black_box(expr), black_box(2.5)))
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_simple_evaluation,
    bench_variable_evaluation,
    bench_complex_evaluation,
    bench_trig_evaluation,
    bench_lambert_w_evaluation,
    bench_x_to_x_evaluation,
    bench_workspace_vs_allocating,
    bench_multiple_x_values,
    bench_expression_sizes,
);
criterion_main!(benches);
