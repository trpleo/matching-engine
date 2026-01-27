# Matching Engine Project Structure

## Directory Layout

```text
matching-engine/
├── Cargo.toml                  # Project configuration and dependencies
├── Cargo.lock                  # Dependency lock file
├── Makefile                    # Build and development commands
├── rustfmt.toml                # Code formatting configuration
│
├── README.md                   # Project documentation
├── PROJECT_STRUCTURE.md        # This file
├── CONFIGURATION.md            # Configuration guide
├── CONTRIBUTING.md             # Contribution guidelines
│
├── docs/
│   └── specs/                  # Future feature specifications
│       ├── L3_FEATURES_SPEC.md # L3 order book features spec
│       └── ANALYSIS_INDEX.md   # Analysis documents index
│
├── src/
│   ├── lib.rs                  # Library root with re-exports
│   │
│   ├── domain/                 # Domain Models (Pure business logic)
│   │   ├── mod.rs
│   │   ├── order.rs            # Order entity with atomic operations
│   │   ├── trade.rs            # Trade entity
│   │   ├── order_book.rs       # Order book data structures
│   │   └── config.rs           # Configuration structs and validation
│   │
│   ├── interfaces/             # Trait Definitions (Contracts)
│   │   ├── mod.rs
│   │   ├── matching_algorithm.rs   # MatchingAlgorithm trait
│   │   └── event_handler.rs        # EventHandler trait & events
│   │
│   ├── engine/                 # Business Logic (Implementations)
│   │   ├── mod.rs
│   │   ├── matching_engine.rs  # Core matching engine
│   │   ├── factory.rs          # Factory and builder patterns
│   │   ├── price_time.rs       # Price/Time (FIFO) algorithm
│   │   ├── pro_rata.rs         # Pro-Rata algorithm
│   │   ├── pro_rata_tob_fifo.rs    # Pro-Rata with Top-of-Book FIFO
│   │   ├── lmm_priority.rs     # LMM Priority (Lead Market Maker)
│   │   └── threshold_pro_rata.rs   # Threshold Pro-Rata algorithm
│   │
│   ├── simd/                   # Performance Optimizations
│   │   ├── mod.rs
│   │   └── price_matcher.rs    # SIMD-accelerated price matching
│   │
│   └── utils/                  # Utilities
│       ├── mod.rs
│       └── numa_detection.rs   # NUMA topology detection
│
├── examples/                   # Usage Examples
│   └── basic_usage.rs          # Basic matching engine usage
│
├── benches/                    # Performance Benchmarks
│   └── matching_benchmark.rs   # Criterion benchmarks
│
└── tests/                      # Integration Tests (empty)
```

## Module Responsibilities

### Domain (`src/domain/`)

**Purpose**: Pure domain models with no external dependencies

**Files**:

- `order.rs`: Order entity with atomic fields for lock-free operations
  - `OrderId`, `Side`, `OrderType`, `TimeInForce`
  - `OrderState` state machine
  - `Order` with atomic `try_fill()` and `try_cancel()`

- `trade.rs`: Immutable trade record
  - `Trade` entity representing a matched trade

- `order_book.rs`: Lock-free order book data structures
  - `OrderBookLevel`: Price level with SegQueue
  - `OrderBookSide`: SkipMap-based sorted price levels
  - `OrderBookSnapshot`: Immutable snapshot for market data

- `config.rs`: Configuration structures
  - `OrderBookConfig`: Main configuration struct
  - `OrderBookType`: Transparent/DarkPool/Hybrid
  - `MatchingAlgorithmType`: Algorithm selection enum
  - Validation rules and preset factory methods

### Interfaces (`src/interfaces/`)

**Purpose**: Trait definitions and contracts

**Files**:

- `matching_algorithm.rs`: Strategy pattern for matching algorithms
  - `MatchingAlgorithm` trait
  - `MatchingConfig` configuration struct

- `event_handler.rs`: Event sourcing interface
  - `EventHandler` trait
  - `OrderEvent` enum (Received, Accepted, Matched, etc.)
  - `NoOpEventHandler`, `LoggingEventHandler` implementations

### Engine (`src/engine/`)

**Purpose**: Core business logic implementations

**Files**:

- `matching_engine.rs`: Main matching engine
  - Order submission with state machine
  - Order cancellation
  - Order book snapshot generation
  - Event emission

- `factory.rs`: Configuration-based engine creation
  - `create_from_config()` factory function
  - `MatchingEngineBuilder` builder pattern
  - Preset configurations (NASDAQ, CME, Eurex, etc.)

- `price_time.rs`: Price/Time (FIFO) matching algorithm
  - Time priority at each price level
  - Optional SIMD optimization

- `pro_rata.rs`: Pro-Rata matching algorithm
  - Proportional allocation by size
  - Configurable minimum quantity
  - Remainder handling

- `pro_rata_tob_fifo.rs`: Pro-Rata with Top-of-Book FIFO
  - First order gets FIFO priority (filled completely)
  - Remaining orders get pro-rata allocation
  - Used by Eurex, ICE Futures

- `lmm_priority.rs`: LMM Priority (Lead Market Maker)
  - Designated market makers get preferential allocation
  - Configurable LMM percentage (e.g., 40%)
  - Remaining quantity distributed pro-rata to all participants

- `threshold_pro_rata.rs`: Threshold Pro-Rata
  - Orders below threshold get FIFO treatment
  - Orders above threshold get pro-rata allocation
  - Protects smaller retail traders

### SIMD (`src/simd/`)

**Purpose**: Platform-specific optimizations

**Files**:

- `price_matcher.rs`: SIMD-accelerated price matching
  - AVX2 implementation for x86_64
  - 4x parallel price comparisons
  - Scalar fallback for other platforms

### Utils (`src/utils/`)

**Purpose**: Utility functions and helpers

**Files**:

- `numa_detection.rs`: NUMA topology detection and CPU affinity (requires `numa` feature)
  - `NumaTopology`: Detects CPU topology and NUMA nodes
  - `NumaNode`: Represents a NUMA node with its cores
  - `pin_current_thread_to_core()`: Pins current thread to a specific CPU core
  - `pin_current_thread_to_node()`: Pins current thread to any core in a NUMA node
  - `get_available_cores()`: Returns list of available CPU cores

## Key Design Patterns

### 1. Strategy Pattern

```rust
pub trait MatchingAlgorithm {
    fn match_order(&self, order: Arc<Order>, opposite_side: &OrderBookSide) -> Vec<Trade>;
}
```

Allows pluggable matching algorithms (Price/Time, Pro-Rata, LMM Priority, etc.)

### 2. Event Sourcing

```rust
pub enum OrderEvent {
    OrderReceived { ... },
    OrderMatched { ... },
    OrderFilled { ... },
    ...
}
```

Complete audit trail for regulatory compliance.

### 3. Lock-Free Concurrency

- `SkipMap` for sorted price levels
- `SegQueue` for FIFO order queues
- `AtomicU64`/`AtomicU8` for order state
- CAS loops for atomic operations

### 4. State Machine

```text
Pending → Accepted → PartiallyFilled → Filled
       ↘ Rejected                    ↗
                   ↘ Cancelled ↗
                   ↘ Expired ↗
```

## Data Flow

```text
1. Order Submission
   └─> MatchingEngine::submit_order()
       ├─> Validation
       ├─> Sequence number assignment
       ├─> Algorithm::match_order()
       │   ├─> SIMD price check (optional)
       │   ├─> Iterate price levels
       │   └─> Generate trades
       ├─> State update (Filled/PartiallyFilled)
       ├─> Add to book (if not filled)
       └─> Emit events

2. Order Cancellation
   └─> MatchingEngine::cancel_order()
       ├─> Lookup in index
       ├─> Atomic state transition
       └─> Emit event

3. Market Data
   └─> MatchingEngine::get_snapshot()
       ├─> Collect top N levels
       ├─> Calculate spread/mid
       └─> Return snapshot
```

## Performance Characteristics

| Operation | Time Complexity | Space Complexity |
|-----------|----------------|------------------|
| Order submission (no match) | O(log n) | O(1) |
| Order matching | O(m log n) | O(m) |
| Order cancellation | O(log n) | O(1) |
| Market data snapshot | O(k) | O(k) |

Where:

- n = number of price levels
- m = number of orders matched
- k = depth of snapshot

## Testing Strategy

- **Unit tests**: In each module (`#[cfg(test)]`)
- **Integration tests**: In `src/lib.rs` (integration_tests module)
- **Benchmarks**: Using Criterion in `benches/`
- **Examples**: Runnable examples in `examples/`

## Building and Running

```bash
# Check compilation
cargo check

# Run tests
cargo test

# Run benchmarks
cargo bench

# Run example
cargo run --example basic_usage

# Build with optimizations
cargo build --release
```

## Features

- `default`: No optional features
- `serde`: Serialization support (serde + serde_json)
- `async`: Tokio integration
- `logging`: Tracing support
- `numa`: NUMA topology detection and CPU affinity (Linux only, uses `core_affinity` crate)
- `avx512`: AVX-512 SIMD optimizations (requires nightly Rust)

## Dependencies

**Core**:

- `crossbeam`, `crossbeam-skiplist`: Lock-free data structures
- `parking_lot`: Faster RwLock
- `rust_decimal`: Precise decimal arithmetic
- `uuid`: Order/trade identification
- `chrono`: Timestamps

**Dev**:

- `criterion`: Benchmarking
- `proptest`: Property-based testing
- `quickcheck`: Randomized testing

## Future Enhancements

- [ ] Memory pool for zero-allocation
- [ ] Async API with Tokio
- [ ] WebSocket market data streaming
- [ ] Persistence layer (PostgreSQL/Kafka)
- [ ] Multi-instrument support
- [ ] Self-trade prevention
- [ ] Price/quantity precision validation
- [ ] L3 order book features (see `docs/specs/L3_FEATURES_SPEC.md`)
