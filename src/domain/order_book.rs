// ============================================================================
// Order Book Domain Model
// ============================================================================

use crossbeam::queue::SegQueue;
use crossbeam_skiplist::SkipMap;
use rust_decimal::Decimal;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use super::{Order, Side};

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

// ============================================================================
// Order Book Level
// ============================================================================

/// Lock-free price level containing orders at a specific price
#[derive(Debug)]
pub struct OrderBookLevel {
    pub price: Decimal,
    /// Lock-free FIFO queue of orders
    pub orders: SegQueue<Arc<Order>>,
    /// Atomic total quantity at this price level
    total_quantity: AtomicU64,
}

impl OrderBookLevel {
    pub fn new(price: Decimal) -> Self {
        Self {
            price,
            orders: SegQueue::new(),
            total_quantity: AtomicU64::new(0),
        }
    }

    pub fn add_order(&self, order: Arc<Order>) {
        let quantity_micros = Self::decimal_to_micros(order.get_remaining_quantity());
        self.total_quantity
            .fetch_add(quantity_micros, Ordering::AcqRel);
        self.orders.push(order);
    }

    pub fn get_total_quantity(&self) -> Decimal {
        let micros = self.total_quantity.load(Ordering::Acquire);
        Self::micros_to_decimal(micros)
    }

    pub fn subtract_quantity(&self, quantity: Decimal) {
        let micros = Self::decimal_to_micros(quantity);
        self.total_quantity.fetch_sub(micros, Ordering::AcqRel);
    }

    pub fn is_empty(&self) -> bool {
        self.orders.is_empty()
    }

    fn decimal_to_micros(value: Decimal) -> u64 {
        use rust_decimal::prelude::ToPrimitive;
        (value * Decimal::from(1_000_000)).to_u64().unwrap_or(0)
    }

    fn micros_to_decimal(micros: u64) -> Decimal {
        Decimal::from(micros) / Decimal::from(1_000_000)
    }
}

// ============================================================================
// Order Book Side
// ============================================================================

/// Lock-free order book side (bids or asks)
/// Uses skip list for sorted price levels
pub struct OrderBookSide {
    /// SkipMap provides lock-free concurrent sorted map
    /// Key: price as i64 micros for sorting
    /// Value: Arc to price level
    pub levels: Arc<SkipMap<i64, Arc<OrderBookLevel>>>,
    pub side: Side,
}

impl OrderBookSide {
    pub fn new(side: Side) -> Self {
        Self {
            levels: Arc::new(SkipMap::new()),
            side,
        }
    }

    /// Add an order to the book side
    pub fn add_order(&self, order: Arc<Order>) {
        let price = order.price.expect("Only limit orders can be added to book");
        let price_micros = self.price_to_key(price);

        // Get or insert price level
        let level = self
            .levels
            .get_or_insert(price_micros, Arc::new(OrderBookLevel::new(price)));

        level.value().add_order(order);
    }

    /// Get the best (top-of-book) price
    pub fn best_price(&self) -> Option<Decimal> {
        match self.side {
            Side::Buy => {
                // Highest bid (last in sorted order)
                self.levels
                    .iter()
                    .next_back()
                    .map(|entry| entry.value().price)
            },
            Side::Sell => {
                // Lowest ask (first in sorted order)
                self.levels.iter().next().map(|entry| entry.value().price)
            },
        }
    }

    /// Get the best price level
    pub fn best_level(&self) -> Option<Arc<OrderBookLevel>> {
        match self.side {
            Side::Buy => self
                .levels
                .iter()
                .next_back()
                .map(|entry| Arc::clone(entry.value())),
            Side::Sell => self
                .levels
                .iter()
                .next()
                .map(|entry| Arc::clone(entry.value())),
        }
    }

    /// Remove empty price levels
    pub fn remove_empty_levels(&self) {
        let mut to_remove = Vec::new();

        for entry in self.levels.iter() {
            if entry.value().is_empty() {
                to_remove.push(*entry.key());
            }
        }

        for key in to_remove {
            self.levels.remove(&key);
        }
    }

    /// Get depth at N levels
    pub fn get_depth(&self, num_levels: usize) -> Vec<(Decimal, Decimal)> {
        let iter: Box<dyn Iterator<Item = _>> = match self.side {
            Side::Buy => Box::new(self.levels.iter().rev()),
            Side::Sell => Box::new(self.levels.iter()),
        };

        iter.take(num_levels)
            .map(|entry| {
                let level = entry.value();
                (level.price, level.get_total_quantity())
            })
            .collect()
    }

    fn price_to_key(&self, price: Decimal) -> i64 {
        use rust_decimal::prelude::ToPrimitive;
        (price * Decimal::from(1_000_000)).to_i64().unwrap_or(0)
    }
}

// ============================================================================
// Order Book Snapshot
// ============================================================================

/// Immutable snapshot of the order book state
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct OrderBookSnapshot {
    pub instrument: String,
    /// Bid levels (price, quantity)
    pub bids: Vec<(Decimal, Decimal)>,
    /// Ask levels (price, quantity)
    pub asks: Vec<(Decimal, Decimal)>,
    /// Current spread (ask - bid)
    pub spread: Option<Decimal>,
    /// Mid price
    pub mid_price: Option<Decimal>,
}

impl OrderBookSnapshot {
    pub fn new(instrument: String) -> Self {
        Self {
            instrument,
            bids: Vec::new(),
            asks: Vec::new(),
            spread: None,
            mid_price: None,
        }
    }

    pub fn with_depth(
        instrument: String,
        bids: Vec<(Decimal, Decimal)>,
        asks: Vec<(Decimal, Decimal)>,
    ) -> Self {
        let spread = match (bids.first(), asks.first()) {
            (Some((bid, _)), Some((ask, _))) => Some(ask - bid),
            _ => None,
        };

        let mid_price = match (bids.first(), asks.first()) {
            (Some((bid, _)), Some((ask, _))) => Some((bid + ask) / Decimal::from(2)),
            _ => None,
        };

        Self {
            instrument,
            bids,
            asks,
            spread,
            mid_price,
        }
    }

    pub fn best_bid(&self) -> Option<Decimal> {
        self.bids.first().map(|(price, _)| *price)
    }

    pub fn best_ask(&self) -> Option<Decimal> {
        self.asks.first().map(|(price, _)| *price)
    }

    pub fn total_bid_quantity(&self) -> Decimal {
        self.bids.iter().map(|(_, qty)| qty).sum()
    }

    pub fn total_ask_quantity(&self) -> Decimal {
        self.asks.iter().map(|(_, qty)| qty).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{OrderType, TimeInForce};

    #[test]
    fn test_order_book_level() {
        let level = OrderBookLevel::new(Decimal::from(50000));

        let order = Arc::new(Order::new(
            "user1".to_string(),
            "BTC-USD".to_string(),
            Side::Buy,
            OrderType::Limit,
            Some(Decimal::from(50000)),
            Decimal::from(1),
            TimeInForce::GoodTillCancel,
        ));

        level.add_order(order);
        assert_eq!(level.get_total_quantity(), Decimal::from(1));
        assert!(!level.is_empty());
    }

    #[test]
    fn test_order_book_side_best_price() {
        let side = OrderBookSide::new(Side::Buy);

        let order1 = Arc::new(Order::new(
            "user1".to_string(),
            "BTC-USD".to_string(),
            Side::Buy,
            OrderType::Limit,
            Some(Decimal::from(50000)),
            Decimal::from(1),
            TimeInForce::GoodTillCancel,
        ));

        let order2 = Arc::new(Order::new(
            "user2".to_string(),
            "BTC-USD".to_string(),
            Side::Buy,
            OrderType::Limit,
            Some(Decimal::from(50100)),
            Decimal::from(1),
            TimeInForce::GoodTillCancel,
        ));

        side.add_order(order1);
        side.add_order(order2);

        // Best bid should be highest price
        assert_eq!(side.best_price(), Some(Decimal::from(50100)));
    }

    #[test]
    fn test_order_book_snapshot() {
        let snapshot = OrderBookSnapshot::with_depth(
            "BTC-USD".to_string(),
            vec![(Decimal::from(50000), Decimal::from(1))],
            vec![(Decimal::from(50100), Decimal::from(2))],
        );

        assert_eq!(snapshot.best_bid(), Some(Decimal::from(50000)));
        assert_eq!(snapshot.best_ask(), Some(Decimal::from(50100)));
        assert_eq!(snapshot.spread, Some(Decimal::from(100)));
        assert_eq!(snapshot.mid_price, Some(Decimal::from(50050)));
    }
}
