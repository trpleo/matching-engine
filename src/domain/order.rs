// ============================================================================
// Order Domain Model
// ============================================================================

use crate::numeric::{Price, Quantity};
use chrono::{DateTime, Utc};
use std::sync::atomic::{AtomicI64, AtomicU8, Ordering};
use std::sync::Arc;
use uuid::Uuid;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

// ============================================================================
// Value Objects
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct OrderId(Uuid);

impl OrderId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl Default for OrderId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum Side {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum OrderType {
    Limit,
    Market,
    StopLimit { trigger_price: Price },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum TimeInForce {
    /// Good Till Cancel - remains active until filled or cancelled
    GoodTillCancel,
    /// Immediate Or Cancel - fill immediately or cancel remainder
    ImmediateOrCancel,
    /// Fill Or Kill - fill entire order immediately or cancel all
    FillOrKill,
    /// Good Till Date - cancel automatically at specified time
    GoodTillDate(DateTime<Utc>),
}

// ============================================================================
// Order State Machine
// ============================================================================

pub mod state {
    #[cfg(feature = "serde")]
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    #[repr(u8)]
    #[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
    pub enum OrderState {
        Pending = 0,
        Accepted = 1,
        PartiallyFilled = 2,
        Filled = 3,
        Cancelled = 4,
        Rejected = 5,
        Expired = 6,
    }

    impl OrderState {
        pub fn from_u8(val: u8) -> Self {
            match val {
                0 => OrderState::Pending,
                1 => OrderState::Accepted,
                2 => OrderState::PartiallyFilled,
                3 => OrderState::Filled,
                4 => OrderState::Cancelled,
                5 => OrderState::Rejected,
                6 => OrderState::Expired,
                _ => OrderState::Rejected,
            }
        }

        pub fn is_terminal(&self) -> bool {
            matches!(
                self,
                OrderState::Filled
                    | OrderState::Cancelled
                    | OrderState::Rejected
                    | OrderState::Expired
            )
        }

        pub fn can_be_cancelled(&self) -> bool {
            matches!(self, OrderState::Accepted | OrderState::PartiallyFilled)
        }
    }

    /// Valid state transitions for the order state machine
    #[derive(Debug, Clone, Copy)]
    pub enum OrderStateTransition {
        Accept,
        Reject,
        PartialFill,
        Fill,
        Cancel,
        Expire,
    }

    impl OrderState {
        pub fn transition(&self, transition: OrderStateTransition) -> Result<OrderState, String> {
            match (self, transition) {
                (OrderState::Pending, OrderStateTransition::Accept) => Ok(OrderState::Accepted),
                (OrderState::Pending, OrderStateTransition::Reject) => Ok(OrderState::Rejected),

                (OrderState::Accepted, OrderStateTransition::PartialFill) => {
                    Ok(OrderState::PartiallyFilled)
                },
                (OrderState::Accepted, OrderStateTransition::Fill) => Ok(OrderState::Filled),
                (OrderState::Accepted, OrderStateTransition::Cancel) => Ok(OrderState::Cancelled),
                (OrderState::Accepted, OrderStateTransition::Expire) => Ok(OrderState::Expired),

                (OrderState::PartiallyFilled, OrderStateTransition::Fill) => Ok(OrderState::Filled),
                (OrderState::PartiallyFilled, OrderStateTransition::Cancel) => {
                    Ok(OrderState::Cancelled)
                },
                (OrderState::PartiallyFilled, OrderStateTransition::Expire) => {
                    Ok(OrderState::Expired)
                },

                _ => Err(format!(
                    "Invalid transition from {:?} via {:?}",
                    self, transition
                )),
            }
        }
    }
}

// ============================================================================
// Lock-Free Order Entity
// ============================================================================

/// Lock-free order with atomic fields for concurrent access
#[derive(Debug)]
pub struct Order {
    pub id: OrderId,
    pub user_id: Arc<String>,
    pub instrument: Arc<String>,
    pub side: Side,
    pub order_type: OrderType,
    pub price: Option<Price>,
    pub quantity: Quantity,
    pub time_in_force: TimeInForce,
    pub timestamp: DateTime<Utc>,

    // Dark pool / visibility controls
    /// If true, order is hidden from order book snapshots (dark pool order)
    pub is_hidden: bool,
    /// For iceberg orders: visible quantity (None = fully visible)
    pub display_quantity: Option<Quantity>,

    // Atomic fields for lock-free updates (stored as raw i64 from FixedDecimal)
    filled_quantity: AtomicI64,
    remaining_quantity: AtomicI64,
    state: AtomicU8,
    sequence_number: AtomicI64,
}

impl Order {
    pub fn new(
        user_id: String,
        instrument: String,
        side: Side,
        order_type: OrderType,
        price: Option<Price>,
        quantity: Quantity,
        time_in_force: TimeInForce,
    ) -> Self {
        Self {
            id: OrderId::new(),
            user_id: Arc::new(user_id),
            instrument: Arc::new(instrument),
            side,
            order_type,
            price,
            quantity,
            time_in_force,
            timestamp: Utc::now(),
            is_hidden: false,
            display_quantity: None,
            filled_quantity: AtomicI64::new(0),
            remaining_quantity: AtomicI64::new(quantity.raw_value()),
            state: AtomicU8::new(state::OrderState::Pending as u8),
            sequence_number: AtomicI64::new(0),
        }
    }

    /// Create a new hidden order (for dark pools)
    pub fn new_hidden(
        user_id: String,
        instrument: String,
        side: Side,
        order_type: OrderType,
        price: Option<Price>,
        quantity: Quantity,
        time_in_force: TimeInForce,
    ) -> Self {
        let mut order = Self::new(
            user_id,
            instrument,
            side,
            order_type,
            price,
            quantity,
            time_in_force,
        );
        order.is_hidden = true;
        order
    }

    /// Create a new iceberg order (partially visible)
    pub fn new_iceberg(
        user_id: String,
        instrument: String,
        side: Side,
        order_type: OrderType,
        price: Option<Price>,
        quantity: Quantity,
        display_quantity: Quantity,
        time_in_force: TimeInForce,
    ) -> Self {
        let mut order = Self::new(
            user_id,
            instrument,
            side,
            order_type,
            price,
            quantity,
            time_in_force,
        );
        order.display_quantity = Some(display_quantity);
        order
    }

    /// Get the visible quantity for this order (respects iceberg display quantity)
    pub fn get_visible_quantity(&self) -> Quantity {
        if self.is_hidden {
            Quantity::ZERO
        } else if let Some(display) = self.display_quantity {
            display.min(self.get_remaining_quantity())
        } else {
            self.get_remaining_quantity()
        }
    }

    // ========================================================================
    // Atomic Getters
    // ========================================================================

    pub fn get_filled_quantity(&self) -> Quantity {
        Quantity::from_raw(self.filled_quantity.load(Ordering::Acquire))
    }

    pub fn get_remaining_quantity(&self) -> Quantity {
        Quantity::from_raw(self.remaining_quantity.load(Ordering::Acquire))
    }

    pub fn get_state(&self) -> state::OrderState {
        state::OrderState::from_u8(self.state.load(Ordering::Acquire))
    }

    pub fn get_sequence_number(&self) -> i64 {
        self.sequence_number.load(Ordering::Acquire)
    }

    // ========================================================================
    // Atomic Operations
    // ========================================================================

    /// Atomically fill a quantity of this order
    /// Returns true if successful, false if insufficient quantity
    pub fn try_fill(&self, quantity: Quantity) -> bool {
        let quantity_raw = quantity.raw_value();

        loop {
            let current_remaining = self.remaining_quantity.load(Ordering::Acquire);

            if current_remaining < quantity_raw {
                return false; // Not enough quantity
            }

            let new_remaining = current_remaining - quantity_raw;

            // Try to update remaining quantity atomically (CAS)
            if self
                .remaining_quantity
                .compare_exchange(
                    current_remaining,
                    new_remaining,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                )
                .is_ok()
            {
                // Update filled quantity
                self.filled_quantity
                    .fetch_add(quantity_raw, Ordering::AcqRel);

                // Update state
                if new_remaining == 0 {
                    self.state
                        .store(state::OrderState::Filled as u8, Ordering::Release);
                } else {
                    self.state
                        .store(state::OrderState::PartiallyFilled as u8, Ordering::Release);
                }

                return true;
            }
            // CAS failed, retry
        }
    }

    /// Atomically cancel this order
    /// Returns true if successfully cancelled
    pub fn try_cancel(&self) -> bool {
        let current_state = self.state.load(Ordering::Acquire);
        let state = state::OrderState::from_u8(current_state);

        if !state.can_be_cancelled() {
            return false;
        }

        self.state
            .compare_exchange(
                current_state,
                state::OrderState::Cancelled as u8,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
    }

    /// Set the sequence number (called by matching engine)
    pub fn set_sequence_number(&self, seq: i64) {
        self.sequence_number.store(seq, Ordering::Release);
    }

    /// Set state (used for accept/reject transitions)
    pub fn set_state(&self, new_state: state::OrderState) {
        self.state.store(new_state as u8, Ordering::Release);
    }

    // ========================================================================
    // Helper Methods
    // ========================================================================

    pub fn is_market_order(&self) -> bool {
        matches!(self.order_type, OrderType::Market)
    }

    pub fn is_limit_order(&self) -> bool {
        matches!(self.order_type, OrderType::Limit)
    }
}

// Clone implementation for Order
impl Clone for Order {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            user_id: Arc::clone(&self.user_id),
            instrument: Arc::clone(&self.instrument),
            side: self.side,
            order_type: self.order_type,
            price: self.price,
            quantity: self.quantity,
            time_in_force: self.time_in_force,
            timestamp: self.timestamp,
            is_hidden: self.is_hidden,
            display_quantity: self.display_quantity,
            filled_quantity: AtomicI64::new(self.filled_quantity.load(Ordering::Acquire)),
            remaining_quantity: AtomicI64::new(self.remaining_quantity.load(Ordering::Acquire)),
            state: AtomicU8::new(self.state.load(Ordering::Acquire)),
            sequence_number: AtomicI64::new(self.sequence_number.load(Ordering::Acquire)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_order_creation() {
        let order = Order::new(
            "user123".to_string(),
            "BTC-USD".to_string(),
            Side::Buy,
            OrderType::Limit,
            Some(Price::from_integer(50000).unwrap()),
            Quantity::from_integer(1).unwrap(),
            TimeInForce::GoodTillCancel,
        );

        assert_eq!(
            order.get_remaining_quantity(),
            Quantity::from_integer(1).unwrap()
        );
        assert_eq!(order.get_filled_quantity(), Quantity::ZERO);
        assert_eq!(order.get_state(), state::OrderState::Pending);
    }

    #[test]
    fn test_atomic_fill() {
        let order = Order::new(
            "user123".to_string(),
            "BTC-USD".to_string(),
            Side::Buy,
            OrderType::Limit,
            Some(Price::from_integer(50000).unwrap()),
            Quantity::from_integer(10).unwrap(),
            TimeInForce::GoodTillCancel,
        );

        assert!(order.try_fill(Quantity::from_integer(3).unwrap()));
        assert_eq!(
            order.get_filled_quantity(),
            Quantity::from_integer(3).unwrap()
        );
        assert_eq!(
            order.get_remaining_quantity(),
            Quantity::from_integer(7).unwrap()
        );
        assert_eq!(order.get_state(), state::OrderState::PartiallyFilled);

        assert!(order.try_fill(Quantity::from_integer(7).unwrap()));
        assert_eq!(order.get_state(), state::OrderState::Filled);
    }

    #[test]
    fn test_overfill_protection() {
        let order = Order::new(
            "user123".to_string(),
            "BTC-USD".to_string(),
            Side::Buy,
            OrderType::Limit,
            Some(Price::from_integer(50000).unwrap()),
            Quantity::from_integer(5).unwrap(),
            TimeInForce::GoodTillCancel,
        );

        assert!(!order.try_fill(Quantity::from_integer(10).unwrap()));
        assert_eq!(order.get_filled_quantity(), Quantity::ZERO);
    }

    #[test]
    fn test_cancel() {
        let order = Order::new(
            "user123".to_string(),
            "BTC-USD".to_string(),
            Side::Buy,
            OrderType::Limit,
            Some(Price::from_integer(50000).unwrap()),
            Quantity::from_integer(1).unwrap(),
            TimeInForce::GoodTillCancel,
        );

        order.set_state(state::OrderState::Accepted);
        assert!(order.try_cancel());
        assert_eq!(order.get_state(), state::OrderState::Cancelled);
    }
}
