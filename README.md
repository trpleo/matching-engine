# Matching Engine

A high-performance, lock-free order matching engine for financial markets with pluggable matching algorithms and SIMD optimizations.

## Features

- **Lock-Free Architecture**: Uses atomic operations and concurrent data structures for maximum throughput
- **Pluggable Matching Algorithms**:
  - Price/Time Priority (FIFO) - Equity markets
  - Pro-Rata - Derivatives markets
  - Pro-Rata with Top-of-Book FIFO - Eurex/ICE style
  - LMM Priority (Lead Market Maker) - Market maker programs
  - Threshold Pro-Rata - Retail protection
  - Easy to add custom algorithms via `MatchingAlgorithm` trait
- **SIMD Optimizations**: AVX2-accelerated price matching on x86_64
- **Event Sourcing**: Complete audit trail for regulatory compliance
- **Sub-Microsecond Latency**: Optimized for low-latency trading
- **Type-Safe**: Leverages Rust's type system for safety

## Architecture

```
matching-engine/
├── src/
│   ├── domain/          # Domain models (Order, Trade, OrderBook, Config)
│   ├── interfaces/      # Trait definitions (MatchingAlgorithm, EventHandler)
│   ├── engine/          # Business logic (MatchingEngine, algorithms)
│   ├── simd/            # SIMD optimizations
│   └── utils/           # Utilities (NUMA detection)
├── examples/            # Usage examples
├── benches/             # Performance benchmarks
└── docs/specs/          # Future feature specifications
```

See [PROJECT_STRUCTURE.md](PROJECT_STRUCTURE.md) for detailed module documentation.

## Usage

Add to your `Cargo.toml`:

```toml
[dependencies]
matching-engine = "0.1.0"
```

### Basic Example

```rust
use matching_engine::prelude::*;
use rust_decimal::Decimal;
use std::sync::Arc;

// Create matching engine
let engine = MatchingEngine::new(
    "BTC-USD".to_string(),
    Box::new(PriceTimePriority::new(true)), // Enable SIMD
    Arc::new(NoOpEventHandler),
);

// Submit sell order
let sell_order = Arc::new(Order::new(
    "user1".to_string(),
    "BTC-USD".to_string(),
    Side::Sell,
    OrderType::Limit,
    Some(Decimal::from(50000)),
    Decimal::from(1),
    TimeInForce::GoodTillCancel,
));

engine.submit_order(sell_order);

// Submit matching buy order
let buy_order = Arc::new(Order::new(
    "user2".to_string(),
    "BTC-USD".to_string(),
    Side::Buy,
    OrderType::Limit,
    Some(Decimal::from(50000)),
    Decimal::from(1),
    TimeInForce::GoodTillCancel,
));

let events = engine.submit_order(buy_order);

// Check if trade occurred
for event in events {
    match event {
        OrderEvent::OrderMatched { trade, .. } => {
            println!("Trade executed: {} @ {}", trade.quantity, trade.price);
        }
        _ => {}
    }
}
```

## Matching Algorithms

### Price/Time Priority (FIFO)

Orders at the same price level are matched in time priority order (first-in-first-out).

```rust
Box::new(PriceTimePriority::new(true)) // Enable SIMD
```

### Pro-Rata

Allocates fills proportionally based on order size at each price level.

```rust
Box::new(ProRata::new(
    Decimal::from(10), // Minimum quantity
    false,              // Top-of-book FIFO
))
```

### Pro-Rata with Top-of-Book FIFO

Hybrid algorithm: first order at each price level gets FIFO priority (filled completely), then remaining quantity is distributed pro-rata among other orders. Used by major derivatives exchanges like Eurex and ICE Futures.

```rust
Box::new(ProRataTobFifo::new(
    Decimal::from(10), // Minimum quantity for pro-rata participation
))
```

**Example allocation:**

- Order A (first): 10 BTC → Gets 10 BTC (FIFO)
- Order B: 100 BTC → Gets pro-rata share
- Order C: 200 BTC → Gets pro-rata share

This rewards both queue position and order size.

### LMM Priority (Lead Market Maker)

Designated market makers receive a preferential percentage allocation before remaining quantity is distributed pro-rata. This incentivizes dedicated liquidity providers to maintain tight spreads.

```rust
Box::new(LmmPriority::new(
    vec!["mm1".to_string(), "mm2".to_string()], // LMM account IDs
    Decimal::from_str_exact("0.4").unwrap(),    // 40% LMM allocation
    Decimal::from(10),                           // Minimum quantity
))
```

**Example allocation (40% LMM, incoming 200 BTC):**

Step 1 - LMM Priority (40% = 80 BTC):
- MM orders get 80 BTC allocated pro-rata among themselves

Step 2 - Pro-Rata (60% = 120 BTC):
- ALL orders (including MMs) get 120 BTC allocated pro-rata

This ensures market makers get preferential treatment while still participating in the general allocation.

### Threshold Pro-Rata

Hybrid algorithm that treats orders differently based on a size threshold: small orders get FIFO treatment, large orders get pro-rata allocation. This protects smaller retail traders while providing size-based allocation for institutional participants.

```rust
Box::new(ThresholdProRata::new(
    Decimal::from(50),  // Threshold: orders below 50 BTC get FIFO
    Decimal::from(10),  // Minimum quantity for pro-rata participation
))
```

**Example allocation (threshold 50 BTC, incoming 200 BTC):**

Orders in book:
- Order A: 20 BTC (below threshold → FIFO)
- Order B: 30 BTC (below threshold → FIFO)
- Order C: 100 BTC (above threshold → pro-rata)
- Order D: 200 BTC (above threshold → pro-rata)

Allocation:
1. FIFO phase: A gets 20 BTC, B gets 30 BTC (total: 50 BTC)
2. Pro-rata phase: Remaining 150 BTC distributed to C and D proportionally

This protects small traders while rewarding large liquidity providers.

## Performance

Benchmarks on Apple M1 (2021):

| Operation | Latency (avg) | Throughput |
|-----------|--------------|------------|
| Order submission (no match) | 200 ns | 5M orders/sec |
| Order matching (1 trade) | 500 ns | 2M orders/sec |
| Order matching (SIMD) | 350 ns | 2.8M orders/sec |
| Pro-rata matching | 1.2 µs | 800K orders/sec |
| Order book snapshot | 150 ns | 6.6M/sec |

Run benchmarks:

```bash
cargo bench
```

## Examples

Run the basic example:

```bash
cargo run --example basic_usage
```

## Testing

Run all tests:

```bash
cargo test
```

Run with features:

```bash
cargo test --all-features
```

## Cargo Features

- `serde`: Enable serialization support (serde + serde_json)
- `async`: Enable async runtime integration (Tokio)
- `logging`: Enable tracing/logging support

See [CONFIGURATION.md](CONFIGURATION.md) for detailed configuration options.

## Design Decisions

### Lock-Free Data Structures

- **SkipMap**: Lock-free sorted map for price levels
- **SegQueue**: Lock-free FIFO queue for orders at each price level
- **Atomic Operations**: CAS loops for order fill operations

### SIMD Optimizations

- **AVX2**: 4x parallel price comparisons
- **Fallback**: Scalar implementation for non-x86_64 platforms

### State Machine

Orders follow a strict state machine:

```
Pending → Accepted → PartiallyFilled → Filled
       ↘ Rejected                    ↗
                   ↘ Cancelled ↗
                   ↘ Expired ↗
```

## License

MIT

## Author

Istvan Papp <istvan.l.papp@gmail.com>

## Contributing

Contributions welcome! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.
