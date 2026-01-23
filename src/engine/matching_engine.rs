// ============================================================================
// Matching Engine
// Core business logic for order matching
// ============================================================================

use crate::domain::order::state::OrderState;
use crate::domain::{Order, OrderBookSide, OrderBookSnapshot, OrderId, Side};
use crate::interfaces::{EventHandler, MatchingAlgorithm, OrderEvent};
use chrono::Utc;
use parking_lot::RwLock;
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Lock-free matching engine with pluggable matching algorithm
pub struct MatchingEngine {
    /// Trading instrument (e.g., "BTC-USD")
    instrument: Arc<String>,

    /// Bid side of the order book
    bids: OrderBookSide,

    /// Ask side of the order book
    asks: OrderBookSide,

    /// Pluggable matching algorithm
    algorithm: Box<dyn MatchingAlgorithm>,

    /// Order index for fast lookups (for cancellations)
    order_index: Arc<RwLock<HashMap<OrderId, Arc<Order>>>>,

    /// Event handler for processing events
    event_handler: Arc<dyn EventHandler>,

    /// Sequence counter for order sequencing
    sequence_counter: AtomicU64,
}

impl MatchingEngine {
    /// Create a new matching engine
    pub fn new(
        instrument: String,
        algorithm: Box<dyn MatchingAlgorithm>,
        event_handler: Arc<dyn EventHandler>,
    ) -> Self {
        Self {
            instrument: Arc::new(instrument),
            bids: OrderBookSide::new(Side::Buy),
            asks: OrderBookSide::new(Side::Sell),
            algorithm,
            order_index: Arc::new(RwLock::new(HashMap::new())),
            event_handler,
            sequence_counter: AtomicU64::new(0),
        }
    }

    /// Submit an order to the matching engine
    pub fn submit_order(&self, order: Arc<Order>) -> Vec<OrderEvent> {
        let mut events = Vec::new();

        // Event: Order received
        events.push(OrderEvent::OrderReceived {
            order_id: order.id,
            timestamp: Utc::now(),
        });

        // Validate order
        if let Err(reason) = self.validate_order(&order) {
            order.set_state(OrderState::Rejected);
            events.push(OrderEvent::OrderRejected {
                order_id: order.id,
                reason,
                timestamp: Utc::now(),
            });
            self.event_handler.on_events(events.clone());
            return events;
        }

        // Assign sequence number
        let seq = self.sequence_counter.fetch_add(1, Ordering::AcqRel);
        order.set_sequence_number(seq);

        // Set state to accepted
        order.set_state(OrderState::Accepted);
        events.push(OrderEvent::OrderAccepted {
            order_id: order.id,
            timestamp: Utc::now(),
        });

        // Match order
        let opposite_side = match order.side {
            Side::Buy => &self.asks,
            Side::Sell => &self.bids,
        };

        let trades = self
            .algorithm
            .match_order(Arc::clone(&order), opposite_side);

        // Generate trade events
        for trade in trades {
            events.push(OrderEvent::OrderMatched {
                trade,
                timestamp: Utc::now(),
            });
        }

        // Check final state
        let remaining = order.get_remaining_quantity();
        let filled = order.get_filled_quantity();

        if remaining == Decimal::ZERO && filled > Decimal::ZERO {
            // Fully filled
            events.push(OrderEvent::OrderFilled {
                order_id: order.id,
                total_filled: filled,
                timestamp: Utc::now(),
            });
        } else if filled > Decimal::ZERO {
            // Partially filled
            events.push(OrderEvent::OrderPartiallyFilled {
                order_id: order.id,
                filled_quantity: filled,
                remaining_quantity: remaining,
                timestamp: Utc::now(),
            });

            // Add remainder to book based on time-in-force
            match order.time_in_force {
                crate::domain::TimeInForce::GoodTillCancel => {
                    self.add_to_book(Arc::clone(&order));
                    events.push(OrderEvent::OrderAddedToBook {
                        order_id: order.id,
                        price: order.price.unwrap(),
                        quantity: remaining,
                        timestamp: Utc::now(),
                    });
                },
                crate::domain::TimeInForce::ImmediateOrCancel => {
                    order.set_state(OrderState::Cancelled);
                    events.push(OrderEvent::OrderCancelled {
                        order_id: order.id,
                        timestamp: Utc::now(),
                    });
                },
                crate::domain::TimeInForce::FillOrKill => {
                    // FOK should have been rejected if can't fill completely
                    order.set_state(OrderState::Cancelled);
                    events.push(OrderEvent::OrderCancelled {
                        order_id: order.id,
                        timestamp: Utc::now(),
                    });
                },
                _ => {},
            }
        } else {
            // Not matched at all, add to book
            self.add_to_book(Arc::clone(&order));
            events.push(OrderEvent::OrderAddedToBook {
                order_id: order.id,
                price: order.price.unwrap(),
                quantity: remaining,
                timestamp: Utc::now(),
            });
        }

        // Emit events
        self.event_handler.on_events(events.clone());

        events
    }

    /// Cancel an order
    pub fn cancel_order(&self, order_id: OrderId) -> Option<OrderEvent> {
        if let Some(order) = self.order_index.write().remove(&order_id) {
            if order.try_cancel() {
                let event = OrderEvent::OrderCancelled {
                    order_id,
                    timestamp: Utc::now(),
                };
                self.event_handler.on_event(event.clone());
                Some(event)
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Get order book snapshot
    pub fn get_snapshot(&self, depth: usize) -> OrderBookSnapshot {
        let bids = self.bids.get_depth(depth);
        let asks = self.asks.get_depth(depth);

        OrderBookSnapshot::with_depth((*self.instrument).clone(), bids, asks)
    }

    /// Get spread
    pub fn get_spread(&self) -> Option<Decimal> {
        match (self.bids.best_price(), self.asks.best_price()) {
            (Some(bid), Some(ask)) => Some(ask - bid),
            _ => None,
        }
    }

    /// Get mid price
    pub fn get_mid_price(&self) -> Option<Decimal> {
        match (self.bids.best_price(), self.asks.best_price()) {
            (Some(bid), Some(ask)) => Some((bid + ask) / Decimal::from(2)),
            _ => None,
        }
    }

    /// Get the instrument name
    pub fn get_instrument(&self) -> &str {
        &self.instrument
    }

    // ========================================================================
    // Private methods
    // ========================================================================

    fn add_to_book(&self, order: Arc<Order>) {
        match order.side {
            Side::Buy => self.bids.add_order(Arc::clone(&order)),
            Side::Sell => self.asks.add_order(Arc::clone(&order)),
        }

        // Index for cancellation
        self.order_index.write().insert(order.id, order);
    }

    fn validate_order(&self, order: &Order) -> Result<(), String> {
        // Basic validation
        if order.quantity <= Decimal::ZERO {
            return Err("Quantity must be positive".to_string());
        }

        if order.is_limit_order() && order.price.is_none() {
            return Err("Limit orders must have a price".to_string());
        }

        if order.is_limit_order() {
            if let Some(price) = order.price {
                if price <= Decimal::ZERO {
                    return Err("Price must be positive".to_string());
                }
            }
        }

        // TODO: Add more validations:
        // - User balance check
        // - Instrument validation
        // - Price/quantity precision check
        // - Self-trade prevention

        Ok(())
    }
}

unsafe impl Send for MatchingEngine {}
unsafe impl Sync for MatchingEngine {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{OrderType, TimeInForce};
    use crate::engine::PriceTimePriority;
    use crate::interfaces::NoOpEventHandler;

    #[test]
    fn test_matching_engine_basic() {
        let engine = MatchingEngine::new(
            "BTC-USD".to_string(),
            Box::new(PriceTimePriority::new(false)),
            Arc::new(NoOpEventHandler),
        );

        // Add sell order
        let sell = Arc::new(Order::new(
            "user1".to_string(),
            "BTC-USD".to_string(),
            Side::Sell,
            OrderType::Limit,
            Some(Decimal::from(50000)),
            Decimal::from(1),
            TimeInForce::GoodTillCancel,
        ));

        engine.submit_order(sell);

        // Add matching buy order
        let buy = Arc::new(Order::new(
            "user2".to_string(),
            "BTC-USD".to_string(),
            Side::Buy,
            OrderType::Limit,
            Some(Decimal::from(50000)),
            Decimal::from(1),
            TimeInForce::GoodTillCancel,
        ));

        let events = engine.submit_order(buy);

        // Should have matched
        assert!(events
            .iter()
            .any(|e| matches!(e, OrderEvent::OrderMatched { .. })));

        // Snapshot should be empty
        let snapshot = engine.get_snapshot(10);
        assert_eq!(snapshot.bids.len(), 0);
        assert_eq!(snapshot.asks.len(), 0);
    }

    #[test]
    fn test_cancel_order() {
        let engine = MatchingEngine::new(
            "BTC-USD".to_string(),
            Box::new(PriceTimePriority::new(false)),
            Arc::new(NoOpEventHandler),
        );

        let order = Arc::new(Order::new(
            "user1".to_string(),
            "BTC-USD".to_string(),
            Side::Buy,
            OrderType::Limit,
            Some(Decimal::from(50000)),
            Decimal::from(1),
            TimeInForce::GoodTillCancel,
        ));

        let order_id = order.id;
        engine.submit_order(order);

        // Cancel order
        let cancel_event = engine.cancel_order(order_id);
        assert!(cancel_event.is_some());
    }

    #[test]
    fn test_order_book_snapshot() {
        let engine = MatchingEngine::new(
            "BTC-USD".to_string(),
            Box::new(PriceTimePriority::new(false)),
            Arc::new(NoOpEventHandler),
        );

        // Add multiple orders
        for i in 0..5 {
            let buy = Arc::new(Order::new(
                format!("user{}", i),
                "BTC-USD".to_string(),
                Side::Buy,
                OrderType::Limit,
                Some(Decimal::from(50000 - i * 100)),
                Decimal::from(1),
                TimeInForce::GoodTillCancel,
            ));
            engine.submit_order(buy);

            let sell = Arc::new(Order::new(
                format!("user{}", i + 10),
                "BTC-USD".to_string(),
                Side::Sell,
                OrderType::Limit,
                Some(Decimal::from(50100 + i * 100)),
                Decimal::from(1),
                TimeInForce::GoodTillCancel,
            ));
            engine.submit_order(sell);
        }

        let snapshot = engine.get_snapshot(3);
        assert_eq!(snapshot.bids.len(), 3);
        assert_eq!(snapshot.asks.len(), 3);
        assert!(snapshot.spread.is_some());
        assert!(snapshot.mid_price.is_some());
    }
}
