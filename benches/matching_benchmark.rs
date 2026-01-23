// ============================================================================
// Matching Engine Benchmarks
// ============================================================================

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use matching_engine::prelude::*;
use rust_decimal::Decimal;
use std::sync::Arc;

fn benchmark_price_time_matching(c: &mut Criterion) {
    let mut group = c.benchmark_group("price_time_matching");

    for num_orders in [100, 1000, 10000].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(num_orders),
            num_orders,
            |b, &num_orders| {
                let engine = MatchingEngine::new(
                    "BTC-USD".to_string(),
                    Box::new(PriceTimePriority::new(false)),
                    Arc::new(NoOpEventHandler),
                );

                // Pre-populate order book
                for i in 0..num_orders / 2 {
                    let sell = Arc::new(Order::new(
                        format!("user{}", i),
                        "BTC-USD".to_string(),
                        Side::Sell,
                        OrderType::Limit,
                        Some(Decimal::from(50000 + i)),
                        Decimal::from(1),
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
                        Some(Decimal::from(50005)),
                        Decimal::from(1),
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

                // Pre-populate with 100 levels
                for i in 0..100 {
                    let sell = Arc::new(Order::new(
                        format!("user{}", i),
                        "BTC-USD".to_string(),
                        Side::Sell,
                        OrderType::Limit,
                        Some(Decimal::from(50000 + i * 10)),
                        Decimal::from(1),
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
                        Some(Decimal::from(50050)),
                        Decimal::from(5),
                        TimeInForce::GoodTillCancel,
                    ));
                    black_box(engine.submit_order(buy));
                });
            },
        );
    }

    group.finish();
}

fn benchmark_pro_rata_matching(c: &mut Criterion) {
    c.bench_function("pro_rata_matching", |b| {
        let engine = MatchingEngine::new(
            "BTC-USD".to_string(),
            Box::new(ProRata::new(Decimal::ZERO, false)),
            Arc::new(NoOpEventHandler),
        );

        // Pre-populate with various order sizes
        for i in 0..50 {
            let sell = Arc::new(Order::new(
                format!("user{}", i),
                "BTC-USD".to_string(),
                Side::Sell,
                OrderType::Limit,
                Some(Decimal::from(50000)),
                Decimal::from((i % 10) + 1),
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
                Some(Decimal::from(50000)),
                Decimal::from(100),
                TimeInForce::GoodTillCancel,
            ));
            black_box(engine.submit_order(buy));
        });
    });
}

fn benchmark_order_book_snapshot(c: &mut Criterion) {
    c.bench_function("order_book_snapshot", |b| {
        let engine = MatchingEngine::new(
            "BTC-USD".to_string(),
            Box::new(PriceTimePriority::new(false)),
            Arc::new(NoOpEventHandler),
        );

        // Pre-populate book
        for i in 0..100 {
            let buy = Arc::new(Order::new(
                format!("buyer{}", i),
                "BTC-USD".to_string(),
                Side::Buy,
                OrderType::Limit,
                Some(Decimal::from(49900 - i * 10)),
                Decimal::from(1),
                TimeInForce::GoodTillCancel,
            ));
            engine.submit_order(buy);

            let sell = Arc::new(Order::new(
                format!("seller{}", i),
                "BTC-USD".to_string(),
                Side::Sell,
                OrderType::Limit,
                Some(Decimal::from(50100 + i * 10)),
                Decimal::from(1),
                TimeInForce::GoodTillCancel,
            ));
            engine.submit_order(sell);
        }

        b.iter(|| {
            black_box(engine.get_snapshot(10));
        });
    });
}

criterion_group!(
    benches,
    benchmark_price_time_matching,
    benchmark_price_time_simd,
    benchmark_pro_rata_matching,
    benchmark_order_book_snapshot
);
criterion_main!(benches);
