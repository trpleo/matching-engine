# Matching Engine Configuration Guide

This guide covers all configuration options for the matching engine, from quick start examples to technical implementation details.

## Table of Contents

1. [Quick Start](#quick-start)
2. [Order Book Types](#order-book-types)
3. [Matching Algorithms](#matching-algorithms)
4. [Configuration Methods](#configuration-methods)
5. [Complete Examples](#complete-examples)
6. [Technical Reference](#technical-reference)
7. [Hardcoded Values](#hardcoded-values)

---

## Quick Start

### Using Preset Configurations

The easiest way to create an order book is using preset configurations:

```rust
use matching_engine::prelude::*;
use std::sync::Arc;

// NASDAQ-style equity exchange
let nasdaq = MatchingEngineBuilder::nasdaq_style("AAPL")
    .build(Arc::new(NoOpEventHandler))
    .unwrap();

// CME-style futures exchange
let cme = MatchingEngineBuilder::cme_style("ES", Decimal::from(10))
    .build(Arc::new(NoOpEventHandler))
    .unwrap();

// Dark pool
let dark = MatchingEngineBuilder::dark_pool_preset("BLOCK-TRADE")
    .build(Arc::new(NoOpEventHandler))
    .unwrap();
```

### Available Presets

| Preset | Order Book | Algorithm | Use Case |
|--------|------------|-----------|----------|
| `nasdaq_style(instrument)` | Transparent | Price/Time (FIFO) | Equity markets |
| `cme_style(instrument, min_qty)` | Transparent | Pro-Rata | Futures markets |
| `eurex_style(instrument, min_qty)` | Transparent | Pro-Rata TOB-FIFO | Derivatives |
| `dark_pool(instrument)` | Dark Pool | Price/Time | Institutional trading |
| `crypto_with_lmm(instrument, accounts, pct)` | Transparent | LMM Priority | Market maker programs |
| `retail_friendly(instrument, threshold)` | Transparent | Threshold Pro-Rata | Retail protection |

---

## Order Book Types

### Transparent Order Book

Full pre-trade transparency (L1/L2/L3 data available). All orders visible in the order book. Used by traditional exchanges (NASDAQ, NYSE, CME).

```rust
let config = OrderBookConfig::new(
    "BTC-USD".to_string(),
    OrderBookType::Transparent,
    MatchingAlgorithmType::PriceTime { use_simd: true },
);
```

### Dark Pool Order Book

No pre-trade transparency (orders hidden). Trades published after execution only. Minimizes market impact for large orders.

```rust
let config = OrderBookConfig::dark_pool("INSTITUTIONAL-BLOCK".to_string());

// Create hidden orders
let hidden_order = Arc::new(Order::new_hidden(
    "institution1".to_string(),
    "INSTITUTIONAL-BLOCK".to_string(),
    Side::Buy,
    OrderType::Limit,
    Some(Decimal::from(100)),
    Decimal::from(10000),
    TimeInForce::GoodTillCancel,
));
```

### Hybrid Order Book

Partial visibility (e.g., iceberg orders with display quantity). Some orders hidden, some visible.

```rust
let config = OrderBookConfig::new(
    "ETH-USD".to_string(),
    OrderBookType::Hybrid,
    MatchingAlgorithmType::PriceTime { use_simd: true },
);

// Iceberg order: 1000 total, only 100 visible
let iceberg = Arc::new(Order::new_iceberg(
    "trader1".to_string(),
    "ETH-USD".to_string(),
    Side::Sell,
    OrderType::Limit,
    Some(Decimal::from(2000)),
    Decimal::from(1000),  // Total quantity
    Decimal::from(100),   // Display quantity
    TimeInForce::GoodTillCancel,
));
```

---

## Matching Algorithms

### 1. Price/Time Priority (FIFO)

Orders at the same price level are matched in time priority order (first-in-first-out).

**Use Case:** Equity markets (NASDAQ, NYSE)

```rust
MatchingAlgorithmType::PriceTime { use_simd: true }
```

### 2. Pro-Rata

Allocates fills proportionally based on order size at each price level.

**Use Case:** Derivatives markets (CME, some crypto exchanges)

**Example allocation (incoming 150 BTC):**
- Level has: [100 BTC, 200 BTC] at $50,000
- Order 1: 150 × (100/300) = 50 BTC
- Order 2: 150 × (200/300) = 100 BTC

```rust
MatchingAlgorithmType::ProRata {
    minimum_quantity: Decimal::from(10),  // Min to participate
    top_of_book_fifo: false,
}
```

### 3. Pro-Rata with Top-of-Book FIFO

First order gets FIFO priority (filled completely), remaining quantity distributed pro-rata.

**Use Case:** Eurex, ICE Futures

```rust
MatchingAlgorithmType::ProRataTobFifo {
    minimum_quantity: Decimal::from(5),
}
```

### 4. LMM Priority (Lead Market Maker)

Designated market makers receive preferential percentage allocation before remaining quantity is distributed pro-rata.

**Use Case:** Exchanges with market maker programs

**Example (40% LMM, incoming 100 BTC):**
- Step 1: 40 BTC allocated to LMMs (pro-rata among LMMs)
- Step 2: 60 BTC allocated to all orders (pro-rata)

```rust
use std::collections::HashSet;

let mut lmm_accounts = HashSet::new();
lmm_accounts.insert("market_maker_1".to_string());
lmm_accounts.insert("market_maker_2".to_string());

MatchingAlgorithmType::LmmPriority {
    lmm_accounts,
    lmm_allocation_pct: Decimal::new(4, 1),  // 0.4 = 40%
    minimum_quantity: Decimal::from(10),
}
```

### 5. Threshold Pro-Rata

Small orders get FIFO treatment, large orders get pro-rata allocation. Protects smaller retail traders.

**Use Case:** Retail-friendly exchanges

**Example (threshold 50 BTC, incoming 100 BTC):**
- Orders < 50 BTC: filled in FIFO order
- Orders >= 50 BTC: remaining quantity distributed pro-rata

```rust
MatchingAlgorithmType::ThresholdProRata {
    threshold: Decimal::from(50),
    minimum_quantity: Decimal::from(10),
}
```

---

## Configuration Methods

### Factory Pattern

```rust
use matching_engine::prelude::*;
use std::sync::Arc;

let config = OrderBookConfig::new(
    "BTC-USD".to_string(),
    OrderBookType::Transparent,
    MatchingAlgorithmType::PriceTime { use_simd: true },
)
.with_tick_size(Decimal::new(1, 2))  // $0.01 tick size
.with_lot_size(Decimal::from(1))     // 1 unit lot size
.with_max_depth(1000);               // Max 1000 price levels

match create_from_config(config, Arc::new(LoggingEventHandler)) {
    Ok(engine) => println!("Engine created successfully"),
    Err(e) => println!("Configuration error: {}", e),
}
```

### Builder Pattern

```rust
let engine = MatchingEngineBuilder::new("BTC-USD")
    .transparent_order_book()
    .pro_rata_matching(Decimal::from(10), false)
    .with_tick_size(Decimal::new(1, 2))
    .with_max_depth(1000)
    .build(Arc::new(NoOpEventHandler))
    .unwrap();
```

### Builder Methods Reference

**Order Book Type:**
- `.transparent_order_book()`
- `.dark_pool()`
- `.hybrid_order_book()`

**Matching Algorithms:**
- `.price_time_matching(use_simd: bool)`
- `.pro_rata_matching(min_qty: Decimal, tob_fifo: bool)`
- `.pro_rata_tob_fifo_matching(min_qty: Decimal)`
- `.lmm_priority_matching(accounts: HashSet<String>, pct: Decimal, min_qty: Decimal)`
- `.threshold_pro_rata_matching(threshold: Decimal, min_qty: Decimal)`

**Additional Configuration:**
- `.with_tick_size(tick: Decimal)`
- `.with_lot_size(lot: Decimal)`
- `.with_max_depth(depth: usize)`

---

## Complete Examples

### Example 1: NASDAQ-Style Exchange

```rust
use matching_engine::prelude::*;
use std::sync::Arc;

fn main() {
    let engine = MatchingEngineBuilder::nasdaq_style("AAPL")
        .with_lot_size(Decimal::from(1))
        .build(Arc::new(LoggingEventHandler))
        .unwrap();

    // Submit sell order
    let sell = Arc::new(Order::new(
        "seller1".to_string(),
        "AAPL".to_string(),
        Side::Sell,
        OrderType::Limit,
        Some(Decimal::from(150)),
        Decimal::from(100),
        TimeInForce::GoodTillCancel,
    ));
    engine.submit_order(sell);

    // Submit matching buy order
    let buy = Arc::new(Order::new(
        "buyer1".to_string(),
        "AAPL".to_string(),
        Side::Buy,
        OrderType::Limit,
        Some(Decimal::from(150)),
        Decimal::from(50),
        TimeInForce::GoodTillCancel,
    ));
    engine.submit_order(buy);

    // Check order book
    let snapshot = engine.get_snapshot(10);
    println!("Best ask: {:?}", snapshot.best_ask());
    println!("Spread: {:?}", snapshot.spread);
}
```

### Example 2: CME-Style Futures with Pro-Rata

```rust
use matching_engine::prelude::*;
use std::sync::Arc;

fn main() {
    let engine = MatchingEngineBuilder::cme_style("ES-202503", Decimal::from(10))
        .build(Arc::new(NoOpEventHandler))
        .unwrap();

    // Add multiple orders at same price
    for (trader, qty) in [("trader1", 50), ("trader2", 100), ("trader3", 150)] {
        engine.submit_order(Arc::new(Order::new(
            trader.to_string(),
            "ES-202503".to_string(),
            Side::Sell,
            OrderType::Limit,
            Some(Decimal::from(4500)),
            Decimal::from(qty),
            TimeInForce::GoodTillCancel,
        )));
    }

    // Incoming buy will be allocated pro-rata
    let buy = Arc::new(Order::new(
        "buyer".to_string(),
        "ES-202503".to_string(),
        Side::Buy,
        OrderType::Limit,
        Some(Decimal::from(4500)),
        Decimal::from(150),
        TimeInForce::GoodTillCancel,
    ));

    let events = engine.submit_order(buy);
    // Allocation: trader1=25, trader2=50, trader3=75 (proportional to order sizes)
}
```

### Example 3: Crypto Exchange with LMM Priority

```rust
use matching_engine::prelude::*;
use std::{sync::Arc, collections::HashSet};

fn main() {
    let mut lmm_accounts = HashSet::new();
    lmm_accounts.insert("market_maker_citadel".to_string());
    lmm_accounts.insert("market_maker_jump".to_string());

    let engine = MatchingEngineBuilder::new("BTC-USD")
        .transparent_order_book()
        .lmm_priority_matching(
            lmm_accounts,
            Decimal::new(4, 1),  // 40% to LMMs
            Decimal::from(10),
        )
        .build(Arc::new(NoOpEventHandler))
        .unwrap();

    // LMM order
    engine.submit_order(Arc::new(Order::new(
        "market_maker_citadel".to_string(),
        "BTC-USD".to_string(),
        Side::Sell,
        OrderType::Limit,
        Some(Decimal::from(50000)),
        Decimal::from(50),
        TimeInForce::GoodTillCancel,
    )));

    // Regular trader order
    engine.submit_order(Arc::new(Order::new(
        "retail_trader".to_string(),
        "BTC-USD".to_string(),
        Side::Sell,
        OrderType::Limit,
        Some(Decimal::from(50000)),
        Decimal::from(100),
        TimeInForce::GoodTillCancel,
    )));

    // When 100 BTC buy comes in:
    // - 40 BTC allocated to LMMs first
    // - 60 BTC allocated pro-rata to all orders
}
```

---

## Technical Reference

### OrderBookConfig Structure

```rust
pub struct OrderBookConfig {
    pub instrument: String,                    // E.g., "BTC-USD"
    pub order_book_type: OrderBookType,        // Transparent/DarkPool/Hybrid
    pub matching_algorithm: MatchingAlgorithmType,
    pub max_depth: Option<usize>,              // Order book depth
    pub tick_size: Option<Decimal>,            // Price precision
    pub lot_size: Option<Decimal>,             // Quantity precision
}
```

### Validation Rules

The system automatically validates:
- Instrument name is not empty
- Tick size > 0 (if specified)
- Lot size > 0 (if specified)
- LMM allocation percentage is 0.0-1.0
- Minimum quantities >= 0
- Thresholds > 0 (for threshold pro-rata)

```rust
let config = OrderBookConfig::new(
    "".to_string(),  // Empty instrument - ERROR
    OrderBookType::Transparent,
    MatchingAlgorithmType::PriceTime { use_simd: true },
);

match config.validate() {
    Ok(_) => println!("Config valid"),
    Err(e) => println!("Validation error: {}", e),
    // Output: "Validation error: Instrument cannot be empty"
}
```

### Key Source Files

| File | Purpose |
|------|---------|
| `src/domain/config.rs` | Configuration structs and validation |
| `src/engine/factory.rs` | Factory and builder patterns |
| `src/engine/price_time.rs` | Price/Time algorithm |
| `src/engine/pro_rata.rs` | Pro-Rata algorithm |
| `src/engine/pro_rata_tob_fifo.rs` | Pro-Rata TOB-FIFO algorithm |
| `src/engine/lmm_priority.rs` | LMM Priority algorithm |
| `src/engine/threshold_pro_rata.rs` | Threshold Pro-Rata algorithm |

### Dependencies for JSON Serialization

The library supports JSON serialization via optional features:

```toml
[dependencies]
matching-engine = { version = "0.1.0", features = ["serde"] }
```

Available serialization dependencies:
- `serde` - Serialization/Deserialization
- `serde_json` - JSON support
- `chrono` - Date/time (with serde)
- `uuid` - Order IDs (with serde)
- `rust_decimal` - Decimal math

### JSON Configuration Example

```json
{
  "instrument": "BTC-USD",
  "order_book_type": "Transparent",
  "matching_algorithm": {
    "type": "ProRata",
    "minimum_quantity": "10.00",
    "top_of_book_fifo": false
  },
  "max_depth": 100,
  "tick_size": "0.01",
  "lot_size": "0.001"
}
```

---

## Hardcoded Values

### Critical Hardcoded Value

**Decimal Precision Constant: `1_000_000`**

| Location | Purpose |
|----------|---------|
| `src/domain/order_book.rs:62` | `decimal_to_micros()` |
| `src/domain/order_book.rs:66` | `micros_to_decimal()` |
| `src/domain/order_book.rs:170` | `price_to_key()` |

This value defines the internal precision for quantity and price representations (microsecond/microcent precision). Any change requires recompilation.

**Recommendation:** Extract to named constant:
```rust
const PRECISION_MULTIPLIER: u64 = 1_000_000;
```

### Configurable Values (Everything Else)

All other values are configurable through `OrderBookConfig`:

| Parameter | Configurable Via |
|-----------|------------------|
| Instrument name | `OrderBookConfig.instrument` |
| Order book type | `OrderBookConfig.order_book_type` |
| Matching algorithm | `OrderBookConfig.matching_algorithm` |
| Max depth | `OrderBookConfig.max_depth` |
| Tick size | `OrderBookConfig.tick_size` |
| Lot size | `OrderBookConfig.lot_size` |
| SIMD usage | `MatchingAlgorithmType::PriceTime { use_simd }` |
| Minimum quantities | Algorithm-specific parameters |
| LMM accounts/percentage | `MatchingAlgorithmType::LmmPriority` |
| Threshold | `MatchingAlgorithmType::ThresholdProRata` |

### Performance Configuration

These configuration options affect performance:
- `use_simd: true` - 2-3x faster price matching (AVX2 on x86_64)
- `max_depth` - Higher values use more memory
- `minimum_quantity` filters - Fewer orders to process

---

## Summary Tables

### Order Book Types

| Type | Pre-Trade Transparency | Use Case |
|------|----------------------|----------|
| **Transparent** | Full (L1/L2/L3) | Traditional exchanges |
| **Dark Pool** | None | Institutional block trades |
| **Hybrid** | Partial (display qty) | Large orders with partial visibility |

### Matching Algorithms

| Algorithm | Allocation Method | Best For |
|-----------|------------------|----------|
| **Price/Time (FIFO)** | First-in-first-out | Equity markets |
| **Pro-Rata** | Size-proportional | Derivatives |
| **Pro-Rata TOB FIFO** | First order FIFO, rest pro-rata | Eurex-style |
| **LMM Priority** | LMMs first, then pro-rata | Market maker programs |
| **Threshold Pro-Rata** | Small=FIFO, Large=pro-rata | Retail protection |
