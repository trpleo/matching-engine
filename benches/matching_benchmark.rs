// ============================================================================
// Matching Engine Benchmarks
// ============================================================================
//
// Benchmark Categories:
// 1. Raw SIMD - Isolates the SIMD price matching component
// 2. Full Matching - End-to-end order matching through the engine
// 3. Algorithm Comparison - Different matching algorithms
// 4. Order Book Operations - Snapshot and other operations
//
// Architecture Notes:
// - x86_64: Uses AVX2 (256-bit, 4x i64 parallel)
// - aarch64: Uses NEON (128-bit, 2x i64 parallel)
// - Other: Scalar fallback
// ============================================================================

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use matching_engine::numeric::{Price, Quantity};
use matching_engine::prelude::*;
use matching_engine::simd::{create_simd_matcher, ScalarMatcher, SimdMatcher};
use std::sync::Arc;

// ============================================================================
// Raw SIMD Benchmarks
// Isolates just the SIMD price crossing detection
// ============================================================================

fn benchmark_simd_price_matcher(c: &mut Criterion) {
    let mut group = c.benchmark_group("simd_price_matcher");

    // Test with different array sizes to see SIMD benefits
    for num_prices in [10, 100, 1000].iter() {
        // Generate price array (raw i64 values)
        let prices: Vec<i64> = (0..*num_prices)
            .map(|i| Price::from_integer(50000 + i * 10).unwrap().raw_value())
            .collect();

        // Test case: buy order that crosses about 25% of prices
        let buy_price = Price::from_integer(50000 + (*num_prices * 10 / 4))
            .unwrap()
            .raw_value();

        // SIMD matcher
        let simd_matcher = create_simd_matcher();
        group.bench_with_input(
            BenchmarkId::new("SIMD", num_prices),
            &(&prices, buy_price),
            |b, (prices, buy_price)| {
                b.iter(|| black_box(simd_matcher.find_crossing_buy_prices(*buy_price, prices)));
            },
        );

        // Scalar comparison
        let scalar_matcher = ScalarMatcher;
        group.bench_with_input(
            BenchmarkId::new("Scalar", num_prices),
            &(&prices, buy_price),
            |b, (prices, buy_price)| {
                b.iter(|| black_box(scalar_matcher.find_crossing_buy_prices(*buy_price, prices)));
            },
        );
    }

    group.finish();
}

// ============================================================================
// Full Matching Engine Benchmarks
// End-to-end order submission and matching
// ============================================================================

fn benchmark_price_time_matching(c: &mut Criterion) {
    let mut group = c.benchmark_group("price_time_matching");

    for num_orders in [100, 1000, 10000].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(num_orders),
            num_orders,
            |b, &num_orders| {
                let engine = MatchingEngine::new(
                    "BTC-USD".to_string(),
                    Box::new(PriceTimePriority::new(true)), // Enable SIMD
                    Arc::new(NoOpEventHandler),
                );

                // Pre-populate order book with sell orders at different prices
                for i in 0..num_orders / 2 {
                    let sell = Arc::new(Order::new(
                        format!("user{}", i),
                        "BTC-USD".to_string(),
                        Side::Sell,
                        OrderType::Limit,
                        Some(Price::from_integer(50000 + i as i64).unwrap()),
                        Quantity::from_integer(1).unwrap(),
                        TimeInForce::GoodTillCancel,
                    ));
                    engine.submit_order(sell);
                }

                b.iter(|| {
                    // Buy order that crosses with first 5 price levels
                    let buy = Arc::new(Order::new(
                        "benchmark_user".to_string(),
                        "BTC-USD".to_string(),
                        Side::Buy,
                        OrderType::Limit,
                        Some(Price::from_integer(50005).unwrap()),
                        Quantity::from_integer(1).unwrap(),
                        TimeInForce::GoodTillCancel,
                    ));
                    black_box(engine.submit_order(buy));
                });
            },
        );
    }

    group.finish();
}

fn benchmark_price_time_simd(c: &mut Criterion) {
    let mut group = c.benchmark_group("price_time_simd_comparison");

    for use_simd in [false, true].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(if *use_simd { "SIMD" } else { "Scalar" }),
            use_simd,
            |b, &use_simd| {
                let engine = MatchingEngine::new(
                    "BTC-USD".to_string(),
                    Box::new(PriceTimePriority::new(use_simd)),
                    Arc::new(NoOpEventHandler),
                );

                // Pre-populate with 100 price levels
                for i in 0..100 {
                    let sell = Arc::new(Order::new(
                        format!("user{}", i),
                        "BTC-USD".to_string(),
                        Side::Sell,
                        OrderType::Limit,
                        Some(Price::from_integer(50000 + i * 10).unwrap()),
                        Quantity::from_integer(1).unwrap(),
                        TimeInForce::GoodTillCancel,
                    ));
                    engine.submit_order(sell);
                }

                b.iter(|| {
                    // Buy order that crosses first 6 levels (50000-50050)
                    let buy = Arc::new(Order::new(
                        "benchmark_user".to_string(),
                        "BTC-USD".to_string(),
                        Side::Buy,
                        OrderType::Limit,
                        Some(Price::from_integer(50050).unwrap()),
                        Quantity::from_integer(5).unwrap(),
                        TimeInForce::GoodTillCancel,
                    ));
                    black_box(engine.submit_order(buy));
                });
            },
        );
    }

    group.finish();
}

// Benchmark where SIMD helps most: order that doesn't cross any prices
// SIMD can quickly determine no match is possible
fn benchmark_simd_no_match(c: &mut Criterion) {
    let mut group = c.benchmark_group("simd_no_match");

    for use_simd in [false, true].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(if *use_simd { "SIMD" } else { "Scalar" }),
            use_simd,
            |b, &use_simd| {
                let engine = MatchingEngine::new(
                    "BTC-USD".to_string(),
                    Box::new(PriceTimePriority::new(use_simd)),
                    Arc::new(NoOpEventHandler),
                );

                // Pre-populate with 1000 sell orders at high prices
                for i in 0..1000 {
                    let sell = Arc::new(Order::new(
                        format!("user{}", i),
                        "BTC-USD".to_string(),
                        Side::Sell,
                        OrderType::Limit,
                        Some(Price::from_integer(60000 + i as i64).unwrap()), // High prices
                        Quantity::from_integer(1).unwrap(),
                        TimeInForce::GoodTillCancel,
                    ));
                    engine.submit_order(sell);
                }

                b.iter(|| {
                    // Buy order below all sell prices - no match possible
                    let buy = Arc::new(Order::new(
                        "benchmark_user".to_string(),
                        "BTC-USD".to_string(),
                        Side::Buy,
                        OrderType::Limit,
                        Some(Price::from_integer(50000).unwrap()), // Below all asks
                        Quantity::from_integer(1).unwrap(),
                        TimeInForce::GoodTillCancel,
                    ));
                    black_box(engine.submit_order(buy));
                });
            },
        );
    }

    group.finish();
}

// ============================================================================
// Algorithm Comparison Benchmarks
// ============================================================================

fn benchmark_pro_rata_matching(c: &mut Criterion) {
    c.bench_function("pro_rata_matching", |b| {
        let engine = MatchingEngine::new(
            "BTC-USD".to_string(),
            Box::new(ProRata::new(Quantity::ZERO, false)),
            Arc::new(NoOpEventHandler),
        );

        // Pre-populate with orders of various sizes at same price
        for i in 0..50 {
            let sell = Arc::new(Order::new(
                format!("user{}", i),
                "BTC-USD".to_string(),
                Side::Sell,
                OrderType::Limit,
                Some(Price::from_integer(50000).unwrap()),
                Quantity::from_integer((i % 10) + 1).unwrap(),
                TimeInForce::GoodTillCancel,
            ));
            engine.submit_order(sell);
        }

        b.iter(|| {
            let buy = Arc::new(Order::new(
                "benchmark_user".to_string(),
                "BTC-USD".to_string(),
                Side::Buy,
                OrderType::Limit,
                Some(Price::from_integer(50000).unwrap()),
                Quantity::from_integer(100).unwrap(),
                TimeInForce::GoodTillCancel,
            ));
            black_box(engine.submit_order(buy));
        });
    });
}

// ============================================================================
// Order Book Operations Benchmarks
// ============================================================================

fn benchmark_order_book_snapshot(c: &mut Criterion) {
    c.bench_function("order_book_snapshot", |b| {
        let engine = MatchingEngine::new(
            "BTC-USD".to_string(),
            Box::new(PriceTimePriority::new(false)),
            Arc::new(NoOpEventHandler),
        );

        // Pre-populate book with 100 levels on each side
        for i in 0i64..100 {
            let buy = Arc::new(Order::new(
                format!("buyer{}", i),
                "BTC-USD".to_string(),
                Side::Buy,
                OrderType::Limit,
                Some(Price::from_integer(49900 - i * 10).unwrap()),
                Quantity::from_integer(1).unwrap(),
                TimeInForce::GoodTillCancel,
            ));
            engine.submit_order(buy);

            let sell = Arc::new(Order::new(
                format!("seller{}", i),
                "BTC-USD".to_string(),
                Side::Sell,
                OrderType::Limit,
                Some(Price::from_integer(50100 + i * 10).unwrap()),
                Quantity::from_integer(1).unwrap(),
                TimeInForce::GoodTillCancel,
            ));
            engine.submit_order(sell);
        }

        b.iter(|| {
            black_box(engine.get_snapshot(10));
        });
    });
}

fn benchmark_order_submission_no_match(c: &mut Criterion) {
    c.bench_function("order_submission_no_match", |b| {
        let engine = MatchingEngine::new(
            "BTC-USD".to_string(),
            Box::new(PriceTimePriority::new(true)),
            Arc::new(NoOpEventHandler),
        );

        b.iter(|| {
            // Submit order that won't match (empty book on other side)
            let sell = Arc::new(Order::new(
                "benchmark_user".to_string(),
                "BTC-USD".to_string(),
                Side::Sell,
                OrderType::Limit,
                Some(Price::from_integer(50000).unwrap()),
                Quantity::from_integer(1).unwrap(),
                TimeInForce::GoodTillCancel,
            ));
            black_box(engine.submit_order(sell));
        });
    });
}

criterion_group!(
    benches,
    benchmark_simd_price_matcher,
    benchmark_price_time_matching,
    benchmark_price_time_simd,
    benchmark_simd_no_match,
    benchmark_pro_rata_matching,
    benchmark_order_book_snapshot,
    benchmark_order_submission_no_match,
);
criterion_main!(benches);
