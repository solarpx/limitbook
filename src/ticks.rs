use rust_decimal::Decimal;

/// A price level in the order book that orders can rest at.
///
/// Ticks represent discrete price points in the book, ensuring all orders
/// are aligned to valid price levels based on the tick_size.
///
/// # Price Normalization
/// - Prices are normalized to valid ticks on creation
/// - Example: With tick_size 0.01
///   - 100.012 normalizes to 100.01
///   - 100.017 normalizes to 100.02
///
/// # Ordering
/// Implements total ordering for use in BTreeMap:
/// - Ordered by price level for efficient best bid/ask lookup
/// - Enables price-time priority matching
///
/// # Example
/// ```
/// # use rust_decimal_macros::dec;
/// # use limitbook::Tick;
/// let tick = Tick::new(dec!(100.012), dec!(0.01)).unwrap();
/// assert_eq!(tick.level(), dec!(100.01));  // Normalized to tick
/// ```
#[derive(Eq, PartialEq, Ord, PartialOrd, Clone)]
pub struct Tick {
    level: Decimal,     // The normalized price level
    tick_size: Decimal, // Minimum price increment
}

impl Tick {
    pub fn new(price: Decimal, tick_size: Decimal) -> eyre::Result<Self> {
        if price <= Decimal::ZERO {
            return Err(eyre::eyre!("Price must be positive"));
        }

        if tick_size <= Decimal::ZERO {
            return Err(eyre::eyre!("Tick size must be positive"));
        }

        let normalized = Self::normalize(price, tick_size);
        Ok(Self {
            level: normalized,
            tick_size,
        })
    }

    // Static method to handle normalization
    fn normalize(price: Decimal, tick_size: Decimal) -> Decimal {
        (price / tick_size).round() * tick_size
    }

    pub fn level(&self) -> Decimal {
        self.level
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec; // for nice decimal literals in tests

    #[test]
    fn test_tick_normalization() {
        let tick_size = dec!(0.01);

        // Should round to nearest tick
        assert_eq!(
            Tick::new(dec!(10.012), tick_size)
                .expect("invalid tick")
                .level(),
            dec!(10.01)
        );
        assert_eq!(
            Tick::new(dec!(10.017), tick_size)
                .expect("invalid tick")
                .level(),
            dec!(10.02)
        );

        // Exact ticks should remain unchanged
        assert_eq!(
            Tick::new(dec!(10.02), tick_size)
                .expect("invalid tick")
                .level(),
            dec!(10.02)
        );
    }
}
