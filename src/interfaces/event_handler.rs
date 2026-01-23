// ============================================================================
// Event Handler Interface
// Defines the contract for handling order and trade events
// ============================================================================

use crate::domain::{OrderId, Trade};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Events emitted by the matching engine
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum OrderEvent {
    /// Order received by the matching engine
    OrderReceived {
        order_id: OrderId,
        timestamp: DateTime<Utc>,
    },

    /// Order accepted and validated
    OrderAccepted {
        order_id: OrderId,
        timestamp: DateTime<Utc>,
    },

    /// Order rejected with reason
    OrderRejected {
        order_id: OrderId,
        reason: String,
        timestamp: DateTime<Utc>,
    },

    /// Order matched, trade generated
    OrderMatched {
        trade: Trade,
        timestamp: DateTime<Utc>,
    },

    /// Order partially filled
    OrderPartiallyFilled {
        order_id: OrderId,
        filled_quantity: Decimal,
        remaining_quantity: Decimal,
        timestamp: DateTime<Utc>,
    },

    /// Order fully filled
    OrderFilled {
        order_id: OrderId,
        total_filled: Decimal,
        timestamp: DateTime<Utc>,
    },

    /// Order cancelled
    OrderCancelled {
        order_id: OrderId,
        timestamp: DateTime<Utc>,
    },

    /// Order expired (GTD orders)
    OrderExpired {
        order_id: OrderId,
        timestamp: DateTime<Utc>,
    },

    /// Order added to book
    OrderAddedToBook {
        order_id: OrderId,
        price: Decimal,
        quantity: Decimal,
        timestamp: DateTime<Utc>,
    },
}

/// Event handler trait for processing matching engine events
/// Implementations can handle logging, metrics, notifications, etc.
pub trait EventHandler: Send + Sync {
    /// Handle an order event
    fn on_event(&self, event: OrderEvent);

    /// Batch event handler (optional optimization)
    fn on_events(&self, events: Vec<OrderEvent>) {
        for event in events {
            self.on_event(event);
        }
    }
}

/// No-op event handler for testing
pub struct NoOpEventHandler;

impl EventHandler for NoOpEventHandler {
    fn on_event(&self, _event: OrderEvent) {
        // Do nothing
    }
}

/// Logging event handler
pub struct LoggingEventHandler;

impl EventHandler for LoggingEventHandler {
    fn on_event(&self, event: OrderEvent) {
        tracing::debug!("Matching engine event: {:?}", event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_noop_handler() {
        let handler = NoOpEventHandler;
        handler.on_event(OrderEvent::OrderReceived {
            order_id: OrderId::new(),
            timestamp: Utc::now(),
        });
        // Should not panic
    }
}
