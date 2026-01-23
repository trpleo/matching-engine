// ============================================================================
// Basic Usage Example
// ============================================================================

use matching_engine::prelude::*;
use rust_decimal::Decimal;
use std::sync::Arc;

fn main() {
    println!("=== Matching Engine Example ===\n");

    // Create matching engine with Price/Time algorithm
    let engine = MatchingEngine::new(
        "BTC-USD".to_string(),
        Box::new(PriceTimePriority::new(true)), // Enable SIMD
        Arc::new(LoggingEventHandler),
    );

    println!("Created matching engine for BTC-USD\n");

    // Add sell orders at different prices
    println!("Adding sell orders...");
    for i in 0..5 {
        let sell = Arc::new(Order::new(
            format!("seller_{}", i),
            "BTC-USD".to_string(),
            Side::Sell,
            OrderType::Limit,
            Some(Decimal::from(50000 + i * 100)),
            Decimal::from(1),
            TimeInForce::GoodTillCancel,
        ));
        engine.submit_order(sell);
    }

    // Add buy orders
    println!("Adding buy orders...");
    for i in 0..5 {
        let buy = Arc::new(Order::new(
            format!("buyer_{}", i),
            "BTC-USD".to_string(),
            Side::Buy,
            OrderType::Limit,
            Some(Decimal::from(49900 - i * 100)),
            Decimal::from(1),
            TimeInForce::GoodTillCancel,
        ));
        engine.submit_order(buy);
    }

    // Get order book snapshot
    println!("\n=== Order Book Snapshot ===");
    let snapshot = engine.get_snapshot(5);

    println!("\nBids:");
    for (price, qty) in &snapshot.bids {
        println!("  {} @ {}", qty, price);
    }

    println!("\nAsks:");
    for (price, qty) in &snapshot.asks {
        println!("  {} @ {}", qty, price);
    }

    println!("\nSpread: {:?}", snapshot.spread);
    println!("Mid Price: {:?}", snapshot.mid_price);

    // Submit a market buy order that will match
    println!("\n=== Submitting Market Order ===");
    let market_buy = Arc::new(Order::new(
        "market_buyer".to_string(),
        "BTC-USD".to_string(),
        Side::Buy,
        OrderType::Limit,
        Some(Decimal::from(50200)), // Will cross first 3 ask levels
        Decimal::from(2),
        TimeInForce::ImmediateOrCancel,
    ));

    let events = engine.submit_order(market_buy);

    println!("\nEvents generated:");
    for event in &events {
        match event {
            OrderEvent::OrderMatched { trade, .. } => {
                println!(
                    "  Trade: {} @ {} (qty: {})",
                    trade.id, trade.price, trade.quantity
                );
            },
            OrderEvent::OrderFilled { order_id, .. } => {
                println!("  Order {} filled", order_id.as_uuid());
            },
            _ => {},
        }
    }

    // Final snapshot
    println!("\n=== Final Order Book ===");
    let final_snapshot = engine.get_snapshot(10);
    println!("Bids: {} levels", final_snapshot.bids.len());
    println!("Asks: {} levels", final_snapshot.asks.len());
    println!("Spread: {:?}", final_snapshot.spread);
}
