use crate::order::{Fill, Order, OrderId, OrderSide, OrderType};
use crate::ticks::Tick;

use rust_decimal::Decimal;
use std::collections::{BTreeMap, HashMap, VecDeque};

// Orders structure with useful metadata
pub struct Orders {
    orders: VecDeque<Order>,
    total_volume: Decimal, // Cache of total volume at this tick
    order_count: usize,    // Cache of number of orders
}

impl Orders {
    fn new() -> Self {
        Self {
            orders: VecDeque::new(),
            total_volume: Decimal::ZERO,
            order_count: 0,
        }
    }

    fn add_order(&mut self, order: Order) {
        self.total_volume += order.quantity;
        self.order_count += 1;
        self.orders.push_back(order);
    }

    fn remove_order(&mut self, order_id: OrderId) -> eyre::Result<Order> {
        if let Some(pos) = self.orders.iter().position(|order| order.id == order_id) {
            let order = self.orders.remove(pos).unwrap();
            self.total_volume -= order.quantity;
            self.order_count -= 1;
            Ok(order)
        } else {
            Err(eyre::eyre!("Order not found in tick level"))
        }
    }
}

/// A Central Limit Order Book (CLOB) implementation with price-time priority matching.
///
/// The OrderBook maintains two sides (bids and asks) using ordered price levels (Ticks).
/// Each price level maintains a FIFO queue of orders for time priority matching.
///
/// # Performance
/// - Price levels: O(log n) lookup using BTreeMap
/// - Order lookup: O(1) using HashMap
/// - Time priority: O(1) using VecDeque
/// - Volume tracking: O(1) using cached totals
///
/// # Data Structures
/// - BTreeMap<Tick, Orders> for price-ordered levels
/// - VecDeque<Order> for time priority within each level
/// - HashMap<OrderId, (OrderSide, Tick)> for O(1) order lookup
///
/// # Example
/// ```
/// # use rust_decimal_macros::dec;
/// # use limitbook::{OrderBook, OrderSide};
/// let mut book = OrderBook::new(dec!(0.01)).unwrap();
///
/// // Add a limit sell order
/// let (sell_id, _) = book.add_limit_order(
///     OrderSide::Sell,
///     dec!(100.00),
///     dec!(10),
/// ).expect("invalid order");
///
/// // Add a matching buy order
/// let (buy_id, fills) = book.add_limit_order(
///     OrderSide::Buy,
///     dec!(100.00),
///     dec!(5),
/// ).expect("invalid order");
/// ```
pub struct OrderBook {
    pub(crate) tick_size: Decimal, // e.g., 0.01
    pub(crate) bids: BTreeMap<Tick, Orders>,
    pub(crate) asks: BTreeMap<Tick, Orders>,
    pub(crate) next_id: OrderId, // Starts at 0 and increments so there is never a collision
    // Add this to track where orders are O(1) performance versus O(log(n))
    pub(crate) order_lookup: HashMap<OrderId, (OrderSide, Tick)>,
    // Add these to track total liquidity
    pub(crate) total_bid_volume: Decimal,
    pub(crate) total_ask_volume: Decimal,
}

impl OrderBook {
    pub fn new(tick_size: Decimal) -> eyre::Result<Self> {
        if tick_size <= Decimal::ZERO {
            return Err(eyre::eyre!("Tick size must be positive"));
        }

        Ok(Self {
            tick_size,
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
            next_id: 0, // Start at 0
            order_lookup: HashMap::new(),
            total_bid_volume: Decimal::ZERO,
            total_ask_volume: Decimal::ZERO,
        })
    }

    // OrderId Incrementer
    fn next_order_id(&mut self) -> OrderId {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// Add a limit order to the book with price-time priority matching.
    ///
    /// # Arguments
    /// * `order_side` - Buy or Sell
    /// * `price` - Limit price for the order
    /// * `quantity` - Size of the order
    ///
    /// # Returns
    /// Returns a tuple of:
    /// * `OrderId` - Unique identifier for the order (always generated, even for immediate fills)
    /// * `Vec<Fill>` - Any fills that occurred during matching
    ///
    /// # Matching Behavior
    /// 1. For Buy Orders:
    ///    - Matches against asks starting at lowest price
    ///    - Matches if limit price >= ask price
    ///    - Remaining quantity added to bid book
    ///    - If price is above best ask, behaves like market order until price no longer crosses
    ///
    /// 2. For Sell Orders:
    ///    - Matches against bids starting at highest price
    ///    - Matches if limit price <= bid price
    ///    - Remaining quantity added to ask book
    ///    - If price is below best bid, behaves like market order until price no longer crosses
    ///
    /// Within each price level, orders are matched in time priority (FIFO).
    /// An OrderId is always generated and returned, even if the order fills immediately.
    ///
    /// # Examples
    /// ```
    /// # use rust_decimal_macros::dec;
    /// # use limitbook::{OrderBook, OrderSide};
    /// # fn main() {
    /// let mut book = OrderBook::new(dec!(0.01)).unwrap();
    ///
    /// // Add a resting limit sell order
    /// let (sell_id, fills) = book.add_limit_order(
    ///     OrderSide::Sell,
    ///     dec!(100.00),
    ///     dec!(10),
    /// ).expect("invalid order");
    /// assert!(fills.is_empty());  // No fills, order rests
    ///
    /// // Add a limit buy that crosses the book (immediate execution)
    /// let (buy_id, fills) = book.add_limit_order(
    ///     OrderSide::Buy,
    ///     dec!(100.00),
    ///     dec!(5),
    /// ).expect("invalid order");
    /// assert_eq!(fills.len(), 1);  // One fill occurred
    /// assert_eq!(fills[0].quantity, dec!(5));
    /// assert_eq!(fills[0].price, dec!(100.00));
    /// assert_eq!(fills[0].maker_order_id, sell_id);
    /// assert_eq!(fills[0].taker_order_id, buy_id);
    /// # }
    /// ```
    pub fn add_limit_order(
        &mut self,
        order_side: OrderSide,
        price: Decimal,
        quantity: Decimal,
    ) -> eyre::Result<(OrderId, Vec<Fill>)> {
        if price <= Decimal::ZERO {
            return Err(eyre::eyre!("Price must be positive"));
        }

        if quantity <= Decimal::ZERO {
            return Err(eyre::eyre!("Quantity must be positive"));
        }

        let order_id = self.next_order_id();
        let mut fills = Vec::new();
        let mut remaining_quantity = quantity;

        // Check if this order crosses the book
        match order_side {
            OrderSide::Buy => {
                while remaining_quantity > Decimal::ZERO {
                    let mut entry = match self.asks.first_entry() {
                        Some(entry) => entry,
                        None => break, // No more asks to match against
                    };

                    if price < entry.key().level() {
                        break; // Price no longer crosses
                    }

                    // Get the price level before mutable borrow
                    let ask_price = entry.key().level();
                    let orders = entry.get_mut();

                    // Match against orders at this level
                    while remaining_quantity > Decimal::ZERO && !orders.orders.is_empty() {
                        let resting_order = orders
                            .orders
                            .front_mut()
                            .expect("Orders empty but should have orders");

                        let fill_quantity = remaining_quantity.min(resting_order.quantity);

                        fills.push(Fill {
                            quantity: fill_quantity,
                            price: ask_price,
                            taker_order_id: order_id,
                            maker_order_id: resting_order.id,
                        });

                        remaining_quantity -= fill_quantity;
                        orders.total_volume -= fill_quantity;
                        self.total_ask_volume -= fill_quantity;

                        if fill_quantity == resting_order.quantity {
                            let removed_order = orders.orders.pop_front().unwrap();
                            orders.order_count -= 1;
                            self.order_lookup.remove(&removed_order.id);
                        }
                    }

                    // Remove empty price level
                    if orders.order_count == 0 {
                        entry.remove();
                    }
                }
            }
            OrderSide::Sell => {
                while remaining_quantity > Decimal::ZERO {
                    let mut entry = match self.bids.last_entry() {
                        Some(entry) => entry,
                        None => break, // No more bids to match against
                    };

                    if price > entry.key().level() {
                        break; // Price no longer crosses
                    }

                    // Get the price level before mutable borrow
                    let bid_price = entry.key().level();
                    let orders = entry.get_mut();

                    // Match against orders at this level
                    while remaining_quantity > Decimal::ZERO && !orders.orders.is_empty() {
                        let resting_order = orders
                            .orders
                            .front_mut()
                            .expect("Orders empty but should have orders");

                        let fill_quantity = remaining_quantity.min(resting_order.quantity);

                        fills.push(Fill {
                            quantity: fill_quantity,
                            price: bid_price,
                            taker_order_id: order_id,
                            maker_order_id: resting_order.id,
                        });

                        remaining_quantity -= fill_quantity;
                        orders.total_volume -= fill_quantity;
                        self.total_bid_volume -= fill_quantity;

                        if fill_quantity == resting_order.quantity {
                            let removed_order = orders.orders.pop_front().unwrap();
                            orders.order_count -= 1;
                            self.order_lookup.remove(&removed_order.id);
                        }
                    }

                    // Remove empty price level
                    if orders.order_count == 0 {
                        entry.remove();
                    }
                }
            }
        }

        // If we have remaining quantity, add it to the book
        if remaining_quantity > Decimal::ZERO {
            let tick = Tick::new(price, self.tick_size).expect("invalid tick");
            match order_side {
                OrderSide::Buy => {
                    self.total_bid_volume += remaining_quantity;
                    self.bids
                        .entry(tick.clone())
                        .or_insert_with(Orders::new)
                        .add_order(
                            Order::new(order_id, remaining_quantity, OrderType::Limit, order_side)
                                .expect("invalid order"),
                        );
                }
                OrderSide::Sell => {
                    self.total_ask_volume += remaining_quantity;
                    self.asks
                        .entry(tick.clone())
                        .or_insert_with(Orders::new)
                        .add_order(
                            Order::new(order_id, remaining_quantity, OrderType::Limit, order_side)
                                .expect("invalid order"),
                        );
                }
            }
            self.order_lookup.insert(order_id, (order_side, tick));
        }

        Ok((order_id, fills))
    }

    /// Cancel an existing limit order.
    ///
    /// # Arguments
    /// * `order_id` - The unique identifier of the order to cancel
    ///
    /// # Returns
    /// * `Ok(())` if the order was successfully cancelled
    /// * `Err` if the order doesn't exist or has already been cancelled/filled
    ///
    /// # Behavior
    /// 1. Removes the order from the book if found
    /// 2. Updates volume tracking at both tick and book level
    /// 3. Cleans up empty price levels
    /// 4. Removes the order from lookup tracking
    ///
    /// # Errors
    /// Returns an error if:
    /// * Order ID doesn't exist in the lookup table
    /// * Order exists in lookup but not found at the specified price level
    ///
    /// # Example
    /// ```
    /// # use rust_decimal_macros::dec;
    /// # use limitbook::{OrderBook, OrderSide};
    /// # use eyre::Result;
    /// # fn main() -> Result<()> {
    /// let mut book = OrderBook::new(dec!(0.01)).unwrap();
    ///
    /// // Add an order
    /// let (order_id, _) = book.add_limit_order(
    ///     OrderSide::Sell,
    ///     dec!(100.00),
    ///     dec!(10),
    /// ).expect("invalid order");
    ///
    /// // Cancel it
    /// book.cancel_limit_order(order_id)?;
    ///
    /// // Trying to cancel again will fail
    /// assert!(book.cancel_limit_order(order_id).is_err());
    /// # Ok(())
    /// # }
    /// ```
    pub fn cancel_limit_order(&mut self, order_id: OrderId) -> eyre::Result<()> {
        // Get the side and tick from our lookup
        let (side, tick) = self
            .order_lookup
            .get(&order_id)
            .ok_or_else(|| eyre::eyre!("Order not found"))?;

        // Get the appropriate book side (bids or asks)
        let book_side = match side {
            OrderSide::Buy => &mut self.bids,
            OrderSide::Sell => &mut self.asks,
        };

        // Get the orders at this tick level
        let orders = book_side
            .get_mut(tick)
            .ok_or_else(|| eyre::eyre!("Tick level not found"))?;

        // Get the removed order so we know its quantity
        let removed_order = orders.remove_order(order_id)?;

        // Update total volume
        match side {
            OrderSide::Buy => self.total_bid_volume -= removed_order.quantity,
            OrderSide::Sell => self.total_ask_volume -= removed_order.quantity,
        }

        // If no orders left at this tick, remove the tick level
        if orders.order_count == 0 {
            book_side.remove(tick);
        }

        // Remove from lookup
        self.order_lookup.remove(&order_id);

        Ok(())
    }

    // Market Order Matching Logic
    //
    // Price-Time Priority is maintained as follows:
    //
    // 1. Price Priority:
    //    - Market Buy orders match against asks in ascending price order (lowest ask first)
    //    - Market Sell orders match against bids in descending price order (highest bid first)
    //    This is achieved using BTreeMap's ordered iteration (first_entry/last_entry)
    //
    // 2. Time Priority:
    //    - Within each price level, orders are stored in a VecDeque
    //    - Orders are matched in FIFO order (front to back)
    //    - New orders are always added to the back (push_back)
    //    - Matches always take from the front (pop_front)
    //
    // Example:
    // For a market buy order of 100 units when the ask book looks like:
    // Price   Quantity    Time
    // 100     50          09:00:00
    // 100     25          09:00:01
    // 101     75          08:59:59
    //
    // The matching would:
    // 1. Fill 50 units at 100 (best price, earliest time)
    // 2. Fill 25 units at 100 (best price, next in time)
    // 3. Fill 25 units at 101 (next price level)
    pub fn execute_market_order(
        &mut self,
        side: OrderSide,
        quantity: Decimal,
    ) -> eyre::Result<Vec<Fill>> {
        // Quick liquidity check first
        let available = match side {
            OrderSide::Buy => self.total_ask_volume,
            OrderSide::Sell => self.total_bid_volume,
        };

        if available < quantity {
            return Err(eyre::eyre!("Insufficient liquidity for market order"));
        }

        let mut remaining_quantity = quantity;
        let mut fills = Vec::new();
        let market_order_id = self.next_order_id();

        // Choose the book side we're matching against
        let book_side = match side {
            OrderSide::Buy => &mut self.asks,  // Lowest asks first
            OrderSide::Sell => &mut self.bids, // Highest bids first
        };

        while remaining_quantity > Decimal::ZERO {
            // Get best price level
            let best_price_entry = match side {
                OrderSide::Buy => book_side.first_entry(), // Lowest ask
                OrderSide::Sell => book_side.last_entry(), // Highest bid
            };

            let mut entry = best_price_entry
                .ok_or_else(|| eyre::eyre!("Insufficient liquidity for market order"))?;

            // Get the price level first
            let price_level = entry.key().level();

            // Then get mutable access to orders
            let orders = entry.get_mut();

            // Match against orders at this level in time priority
            while remaining_quantity > Decimal::ZERO && !orders.orders.is_empty() {
                let resting_order = orders
                    .orders
                    .front_mut()
                    .ok_or_else(|| eyre::eyre!("No orders at price level"))?;

                let fill_quantity = remaining_quantity.min(resting_order.quantity);

                fills.push(Fill {
                    quantity: fill_quantity,
                    price: price_level, // Use stored price_level instead of entry.key()
                    taker_order_id: market_order_id,
                    maker_order_id: resting_order.id,
                });

                // Update quantities and totals
                remaining_quantity -= fill_quantity;
                orders.total_volume -= fill_quantity;
                match side {
                    OrderSide::Buy => self.total_ask_volume -= fill_quantity,
                    OrderSide::Sell => self.total_bid_volume -= fill_quantity,
                }

                // Remove filled order from lookup and book
                if fill_quantity == resting_order.quantity {
                    let removed_order = orders.orders.pop_front().unwrap();
                    orders.order_count -= 1;
                    self.order_lookup.remove(&removed_order.id);
                }
            }

            // Remove empty price levels
            if orders.order_count == 0 {
                entry.remove();
            }
        }

        Ok(fills)
    }

    /// Helpers
    /// Get the best (highest) bid price if any bids exist
    pub fn best_bid(&self) -> Option<Decimal> {
        self.bids.last_key_value().map(|(tick, _)| tick.level())
    }

    /// Get the best (lowest) ask price if any asks exist
    pub fn best_ask(&self) -> Option<Decimal> {
        self.asks.first_key_value().map(|(tick, _)| tick.level())
    }

    /// Get the current spread (best_ask - best_bid)
    /// Returns None if either side is empty
    pub fn spread(&self) -> Option<Decimal> {
        match (self.best_ask(), self.best_bid()) {
            (Some(ask), Some(bid)) => Some(ask - bid),
            _ => None,
        }
    }

    /// Get the best bid and ask prices
    /// Returns (bid, ask) tuple, either value may be None
    pub fn best_prices(&self) -> (Option<Decimal>, Option<Decimal>) {
        (self.best_bid(), self.best_ask())
    }

    /// Get the volume available at the best bid
    pub fn best_bid_volume(&self) -> Option<Decimal> {
        self.bids
            .last_key_value()
            .map(|(_, orders)| orders.total_volume)
    }

    /// Get the volume available at the best ask
    pub fn best_ask_volume(&self) -> Option<Decimal> {
        self.asks
            .first_key_value()
            .map(|(_, orders)| orders.total_volume)
    }
}

// tests
#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_add_limit_order() {
        let mut book = OrderBook::new(dec!(0.01)).expect("tick spacing must be positive"); // 0.01 tick size

        // Add a buy order
        let (order_id, fills) = book
            .add_limit_order(
                OrderSide::Buy,
                dec!(100.00), // Price
                dec!(10),     // 10 units
            )
            .expect("invalid order");

        assert!(fills.is_empty()); // No fills yet as book was empty
        assert_eq!(book.bids.len(), 1); // One price level
        assert_eq!(book.asks.len(), 0); // No asks

        // Verify the order is in the book at the right price
        let tick = Tick::new(dec!(100.00), dec!(0.01)).expect("invalid tick");
        let orders = book.bids.get(&tick).unwrap();
        assert_eq!(orders.order_count, 1);
        assert_eq!(orders.total_volume, dec!(10));

        // Verify order_lookup
        assert!(book.order_lookup.contains_key(&order_id));
        let (side, stored_tick) = book.order_lookup.get(&order_id).unwrap();
        assert_eq!(*side, OrderSide::Buy);
        assert_eq!(stored_tick.level(), dec!(100.00));
        assert_eq!(book.order_lookup.len(), 1);

        // Verify total volumes
        assert_eq!(book.total_bid_volume, dec!(10));
        assert_eq!(book.total_ask_volume, dec!(0));

        // Add a sell order and check both sides
        let (_sell_id, _) = book
            .add_limit_order(OrderSide::Sell, dec!(101.00), dec!(20))
            .expect("invalid order");

        assert_eq!(book.total_bid_volume, dec!(10));
        assert_eq!(book.total_ask_volume, dec!(20));
    }

    #[test]
    fn test_add_limit_order_with_fills() {
        let mut book = OrderBook::new(dec!(0.01)).expect("tick spacing must be positive");

        // Create initial book state with some asks
        let (sell_id1, _) = book
            .add_limit_order(OrderSide::Sell, dec!(100.00), dec!(50))
            .expect("invalid order"); // Best price
        let (sell_id2, _) = book
            .add_limit_order(OrderSide::Sell, dec!(100.00), dec!(25))
            .expect("invalid order"); // Same price, later time
        let (sell_id3, _) = book
            .add_limit_order(OrderSide::Sell, dec!(101.00), dec!(75))
            .expect("invalid order"); // Worse price

        // Verify initial state
        assert_eq!(book.total_ask_volume, dec!(150));

        // Add a limit buy that crosses the book
        let (buy_id, fills) = book
            .add_limit_order(
                OrderSide::Buy,
                dec!(101.00), // Willing to pay up to 101.00
                dec!(100),    // Want 100 units
            )
            .expect("invalid order");

        // Should get same fills as market order test
        assert_eq!(fills.len(), 3);

        // First fill should be earliest order at best price
        assert_eq!(fills[0].quantity, dec!(50));
        assert_eq!(fills[0].price, dec!(100.00));
        assert_eq!(fills[0].maker_order_id, sell_id1);
        assert_eq!(fills[0].taker_order_id, buy_id);

        // Second fill should be next order at same price
        assert_eq!(fills[1].quantity, dec!(25));
        assert_eq!(fills[1].price, dec!(100.00));
        assert_eq!(fills[1].maker_order_id, sell_id2);
        assert_eq!(fills[1].taker_order_id, buy_id);

        // Third fill should be partial fill at worse price
        assert_eq!(fills[2].quantity, dec!(25));
        assert_eq!(fills[2].price, dec!(101.00));
        assert_eq!(fills[2].maker_order_id, sell_id3);
        assert_eq!(fills[2].taker_order_id, buy_id);

        // Verify book state after fills
        assert_eq!(book.total_ask_volume, dec!(50)); // 75 - 25 = 50 remaining at 101.00
        assert!(!book.order_lookup.contains_key(&sell_id1)); // Fully filled orders should be removed
        assert!(!book.order_lookup.contains_key(&sell_id2));
        assert!(book.order_lookup.contains_key(&sell_id3)); // Partially filled order should remain

        // Add a limit buy that doesn't cross
        let (buy_id2, fills2) = book
            .add_limit_order(
                OrderSide::Buy,
                dec!(99.00), // Below best ask
                dec!(25),
            )
            .expect("invalid order");

        // Should get no fills
        assert!(fills2.is_empty());
        assert_eq!(book.total_bid_volume, dec!(25)); // Should rest in book
        assert!(book.order_lookup.contains_key(&buy_id2));
    }

    #[test]
    fn test_cancel_limit_order() {
        let mut book = OrderBook::new(dec!(0.01)).expect("tick spacing must be positive");

        // Add a few orders to create a known state
        let (buy_id1, _) = book
            .add_limit_order(OrderSide::Buy, dec!(100.00), dec!(10))
            .expect("invalid order");
        let (buy_id2, _) = book
            .add_limit_order(OrderSide::Buy, dec!(100.00), dec!(20))
            .expect("invalid order"); // same tick level
        let (_sell_id1, _) = book
            .add_limit_order(OrderSide::Sell, dec!(101.00), dec!(15))
            .expect("invalid order");

        // Initial state verification
        assert_eq!(book.bids.len(), 1); // one tick level
        let tick = Tick::new(dec!(100.00), dec!(0.01)).expect("invalid tick");
        assert_eq!(book.bids.get(&tick).unwrap().order_count, 2);
        assert_eq!(book.bids.get(&tick).unwrap().total_volume, dec!(30));
        assert_eq!(book.total_bid_volume, dec!(30));
        assert_eq!(book.total_ask_volume, dec!(15));

        // Cancel first buy order
        let result = book.cancel_limit_order(buy_id1);
        assert!(result.is_ok());

        // Verify state after first cancel
        assert_eq!(book.bids.get(&tick).unwrap().order_count, 1);
        assert_eq!(book.bids.get(&tick).unwrap().total_volume, dec!(20));
        assert!(!book.order_lookup.contains_key(&buy_id1));
        assert_eq!(book.total_bid_volume, dec!(20)); // Decreased by 10
        assert_eq!(book.total_ask_volume, dec!(15)); // Unchanged

        // Cancel second buy order
        let result = book.cancel_limit_order(buy_id2);
        assert!(result.is_ok());

        // Verify tick level is removed when empty
        assert!(!book.bids.contains_key(&tick));
        assert!(!book.order_lookup.contains_key(&buy_id2));
        assert_eq!(book.total_bid_volume, dec!(0)); // All bids cancelled
        assert_eq!(book.total_ask_volume, dec!(15)); // Asks unchanged

        // Error cases
        let result = book.cancel_limit_order(999); // non-existent order
        assert!(result.is_err());

        let result = book.cancel_limit_order(buy_id1); // already cancelled order
        assert!(result.is_err());

        // Verify totals unchanged after failed cancels
        assert_eq!(book.total_bid_volume, dec!(0));
        assert_eq!(book.total_ask_volume, dec!(15));
    }

    #[test]
    fn test_market_order_price_time_priority() {
        let mut book = OrderBook::new(dec!(0.01)).expect("tick spacing must be positive");

        // Create ask book with multiple price levels and times
        let (sell_id1, _) = book
            .add_limit_order(OrderSide::Sell, dec!(100.00), dec!(50))
            .expect("invalid order"); // Best price
        let (sell_id2, _) = book
            .add_limit_order(OrderSide::Sell, dec!(100.00), dec!(25))
            .expect("invalid order"); // Same price, later time
        let (sell_id3, _) = book
            .add_limit_order(OrderSide::Sell, dec!(101.00), dec!(75))
            .expect("invalid order"); // Worse price

        // Verify initial state
        assert_eq!(book.total_ask_volume, dec!(150));

        // Execute market buy for 100 units
        let fills = book
            .execute_market_order(OrderSide::Buy, dec!(100))
            .expect("Market order should execute");

        // Should get 3 fills
        assert_eq!(fills.len(), 3);

        // First fill should be earliest order at best price
        assert_eq!(fills[0].quantity, dec!(50));
        assert_eq!(fills[0].price, dec!(100.00));
        assert_eq!(fills[0].maker_order_id, sell_id1);

        // Second fill should be next order at same price
        assert_eq!(fills[1].quantity, dec!(25));
        assert_eq!(fills[1].price, dec!(100.00));
        assert_eq!(fills[1].maker_order_id, sell_id2);

        // Third fill should be partial fill at worse price
        assert_eq!(fills[2].quantity, dec!(25));
        assert_eq!(fills[2].price, dec!(101.00));
        assert_eq!(fills[2].maker_order_id, sell_id3);

        // Verify book state after fills
        assert_eq!(book.total_ask_volume, dec!(50)); // 75 - 25 = 50 remaining
        assert!(!book.order_lookup.contains_key(&sell_id1)); // Fully filled orders should be removed
        assert!(!book.order_lookup.contains_key(&sell_id2));
        assert!(book.order_lookup.contains_key(&sell_id3)); // Partially filled order should remain
    }

    #[test]
    fn test_price_helpers() {
        let mut book = OrderBook::new(dec!(0.01)).expect("tick spacing must be positive");

        // Empty book
        assert_eq!(book.best_bid(), None);
        assert_eq!(book.best_ask(), None);
        assert_eq!(book.spread(), None);

        // Add some orders
        book.add_limit_order(OrderSide::Buy, dec!(100.00), dec!(10))
            .expect("invalid order");
        book.add_limit_order(OrderSide::Buy, dec!(99.00), dec!(20))
            .expect("invalid order");
        book.add_limit_order(OrderSide::Sell, dec!(101.00), dec!(15))
            .expect("invalid order");
        book.add_limit_order(OrderSide::Sell, dec!(102.00), dec!(25))
            .expect("invalid order");

        // Check prices
        assert_eq!(book.best_bid(), Some(dec!(100.00)));
        assert_eq!(book.best_ask(), Some(dec!(101.00)));
        assert_eq!(book.spread(), Some(dec!(1.00)));

        // Check volumes
        assert_eq!(book.best_bid_volume(), Some(dec!(10)));
        assert_eq!(book.best_ask_volume(), Some(dec!(15)));

        // Check best prices tuple
        let (bid, ask) = book.best_prices();
        assert_eq!(bid, Some(dec!(100.00)));
        assert_eq!(ask, Some(dec!(101.00)));
    }
}
