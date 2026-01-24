// ============================================================================
// Trade Domain Model
// ============================================================================

use crate::numeric::{NumericResult, Price, Quantity};
use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::OrderId;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Represents a matched trade between two orders
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Trade {
    /// Unique trade identifier
    pub id: Uuid,

    /// Trading instrument
    pub instrument: String,

    /// Order ID of the passive order (resting in book)
    pub maker_order_id: OrderId,

    /// Order ID of the aggressive order (incoming)
    pub taker_order_id: OrderId,

    /// Execution price
    pub price: Price,

    /// Executed quantity
    pub quantity: Quantity,

    /// Trade timestamp
    pub timestamp: DateTime<Utc>,
}

impl Trade {
    pub fn new(
        instrument: String,
        maker_order_id: OrderId,
        taker_order_id: OrderId,
        price: Price,
        quantity: Quantity,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            instrument,
            maker_order_id,
            taker_order_id,
            price,
            quantity,
            timestamp: Utc::now(),
        }
    }

    /// Calculate the notional value of the trade (price * quantity)
    ///
    /// Returns a Result because multiplication can overflow.
    pub fn notional_value(&self) -> NumericResult<Price> {
        self.price.checked_mul(self.quantity)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trade_creation() {
        let trade = Trade::new(
            "BTC-USD".to_string(),
            OrderId::new(),
            OrderId::new(),
            Price::from_integer(50000).unwrap(),
            Quantity::from_integer(1).unwrap(),
        );

        assert_eq!(trade.instrument, "BTC-USD");
        assert_eq!(trade.price, Price::from_integer(50000).unwrap());
        assert_eq!(trade.quantity, Quantity::from_integer(1).unwrap());
        assert_eq!(
            trade.notional_value().unwrap(),
            Price::from_integer(50000).unwrap()
        );
    }

    #[test]
    fn test_notional_value_with_fractional() {
        let trade = Trade::new(
            "BTC-USD".to_string(),
            OrderId::new(),
            OrderId::new(),
            Price::from_parts(100, 500_000_000).unwrap(), // 100.5
            Quantity::from_integer(2).unwrap(),
        );

        // 100.5 * 2 = 201.0
        assert_eq!(
            trade.notional_value().unwrap(),
            Price::from_integer(201).unwrap()
        );
    }
}
