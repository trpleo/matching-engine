// ============================================================================
// Order Book Domain Model
// ============================================================================

use crate::numeric::{Price, Quantity};
use crossbeam::queue::SegQueue;
use crossbeam_skiplist::SkipMap;
use std::sync::atomic::{AtomicI64, Ordering};
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
    pub price: Price,
    /// Lock-free FIFO queue of orders
    pub orders: SegQueue<Arc<Order>>,
    /// Atomic total quantity at this price level (stored as raw i64)
    total_quantity: AtomicI64,
}

impl OrderBookLevel {
    pub fn new(price: Price) -> Self {
        Self {
            price,
            orders: SegQueue::new(),
            total_quantity: AtomicI64::new(0),
        }
    }

    pub fn add_order(&self, order: Arc<Order>) {
        let quantity_raw = order.get_remaining_quantity().raw_value();
        self.total_quantity
            .fetch_add(quantity_raw, Ordering::AcqRel);
        self.orders.push(order);
    }

    pub fn get_total_quantity(&self) -> Quantity {
        Quantity::from_raw(self.total_quantity.load(Ordering::Acquire))
    }

    pub fn subtract_quantity(&self, quantity: Quantity) {
        self.total_quantity
            .fetch_sub(quantity.raw_value(), Ordering::AcqRel);
    }

    pub fn is_empty(&self) -> bool {
        self.orders.is_empty()
    }
}

// ============================================================================
// Order Book Side
// ============================================================================

/// Lock-free order book side (bids or asks)
/// Uses skip list for sorted price levels
pub struct OrderBookSide {
    /// SkipMap provides lock-free concurrent sorted map
    /// Key: price as raw i64 for sorting
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
        let price_key = price.raw_value();

        // Get or insert price level
        let level = self
            .levels
            .get_or_insert(price_key, Arc::new(OrderBookLevel::new(price)));

        level.value().add_order(order);
    }

    /// Get the best (top-of-book) price
    pub fn best_price(&self) -> Option<Price> {
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
    pub fn get_depth(&self, num_levels: usize) -> Vec<(Price, Quantity)> {
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
    pub bids: Vec<(Price, Quantity)>,
    /// Ask levels (price, quantity)
    pub asks: Vec<(Price, Quantity)>,
    /// Current spread (ask - bid)
    pub spread: Option<Price>,
    /// Mid price
    pub mid_price: Option<Price>,
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
        bids: Vec<(Price, Quantity)>,
        asks: Vec<(Price, Quantity)>,
    ) -> Self {
        let spread = match (bids.first(), asks.first()) {
            (Some((bid, _)), Some((ask, _))) => ask.checked_sub(*bid).ok(),
            _ => None,
        };

        let mid_price = match (bids.first(), asks.first()) {
            (Some((bid, _)), Some((ask, _))) => {
                // (bid + ask) / 2
                bid.checked_add(*ask)
                    .ok()
                    .and_then(|sum| sum.checked_mul_int(1).ok()) // identity, just to get the value
                    .map(|sum| Price::from_raw(sum.raw_value() / 2))
            },
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

    pub fn best_bid(&self) -> Option<Price> {
        self.bids.first().map(|(price, _)| *price)
    }

    pub fn best_ask(&self) -> Option<Price> {
        self.asks.first().map(|(price, _)| *price)
    }

    pub fn total_bid_quantity(&self) -> Quantity {
        self.bids
            .iter()
            .fold(Quantity::ZERO, |acc, (_, qty)| acc + *qty)
    }

    pub fn total_ask_quantity(&self) -> Quantity {
        self.asks
            .iter()
            .fold(Quantity::ZERO, |acc, (_, qty)| acc + *qty)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{OrderType, TimeInForce};

    #[test]
    fn test_order_book_level() {
        let level = OrderBookLevel::new(Price::from_integer(50000).unwrap());

        let order = Arc::new(Order::new(
            "user1".to_string(),
            "BTC-USD".to_string(),
            Side::Buy,
            OrderType::Limit,
            Some(Price::from_integer(50000).unwrap()),
            Quantity::from_integer(1).unwrap(),
            TimeInForce::GoodTillCancel,
        ));

        level.add_order(order);
        assert_eq!(
            level.get_total_quantity(),
            Quantity::from_integer(1).unwrap()
        );
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
            Some(Price::from_integer(50000).unwrap()),
            Quantity::from_integer(1).unwrap(),
            TimeInForce::GoodTillCancel,
        ));

        let order2 = Arc::new(Order::new(
            "user2".to_string(),
            "BTC-USD".to_string(),
            Side::Buy,
            OrderType::Limit,
            Some(Price::from_integer(50100).unwrap()),
            Quantity::from_integer(1).unwrap(),
            TimeInForce::GoodTillCancel,
        ));

        side.add_order(order1);
        side.add_order(order2);

        // Best bid should be highest price
        assert_eq!(side.best_price(), Some(Price::from_integer(50100).unwrap()));
    }

    #[test]
    fn test_order_book_snapshot() {
        let snapshot = OrderBookSnapshot::with_depth(
            "BTC-USD".to_string(),
            vec![(
                Price::from_integer(50000).unwrap(),
                Quantity::from_integer(1).unwrap(),
            )],
            vec![(
                Price::from_integer(50100).unwrap(),
                Quantity::from_integer(2).unwrap(),
            )],
        );

        assert_eq!(
            snapshot.best_bid(),
            Some(Price::from_integer(50000).unwrap())
        );
        assert_eq!(
            snapshot.best_ask(),
            Some(Price::from_integer(50100).unwrap())
        );
        assert_eq!(snapshot.spread, Some(Price::from_integer(100).unwrap()));
        assert_eq!(
            snapshot.mid_price,
            Some(Price::from_integer(50050).unwrap())
        );
    }
}
