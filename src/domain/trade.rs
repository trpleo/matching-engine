// ============================================================================
// Trade Domain Model
// ============================================================================

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
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
    pub price: Decimal,

    /// Executed quantity
    pub quantity: Decimal,

    /// Trade timestamp
    pub timestamp: DateTime<Utc>,
}

impl Trade {
    pub fn new(
        instrument: String,
        maker_order_id: OrderId,
        taker_order_id: OrderId,
        price: Decimal,
        quantity: Decimal,
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

    /// Calculate the notional value of the trade
    pub fn notional_value(&self) -> Decimal {
        self.price * self.quantity
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
            Decimal::from(50000),
            Decimal::from(1),
        );

        assert_eq!(trade.instrument, "BTC-USD");
        assert_eq!(trade.price, Decimal::from(50000));
        assert_eq!(trade.quantity, Decimal::from(1));
        assert_eq!(trade.notional_value(), Decimal::from(50000));
    }
}
