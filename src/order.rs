use rust_decimal::Decimal;

/// Unique identifier for orders. Implemented as a simple incrementing counter.
/// This is sufficient for an in-memory order book as IDs never overlap
/// and we maintain strict sequence.
pub type OrderId = u64;

/// The type of order, determining how it will be processed in the book.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum OrderType {
    Limit,
    Market,
}

/// The side of the order, indicating whether it's buying or selling.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum OrderSide {
    Buy,
    Sell,
}

/// Represents an order in the book, containing all necessary information
/// for matching and execution.
///
/// # Time Priority
/// Time priority is maintained by the order of insertion in the VecDeque
/// at each price level, implementing FIFO matching naturally through
/// the data structure.
///
/// # Fields
/// All fields are immutable after creation to maintain order integrity.
pub struct Order {
    pub id: OrderId,           // Unique identifier
    pub quantity: Decimal,     // Size of order
    pub order_type: OrderType, // Limit/Market
    pub order_side: OrderSide, // Buy/Sell
}

impl Order {
    pub fn new(
        id: OrderId,
        quantity: Decimal,
        order_type: OrderType,
        order_side: OrderSide,
    ) -> eyre::Result<Self> {
        if quantity <= Decimal::ZERO {
            return Err(eyre::eyre!("Quantity must be positive"));
        }

        Ok(Self {
            id,
            quantity,
            order_type,
            order_side,
        })
    }
}

/// Represents a match between two orders in the book.
///
/// A Fill is generated when two orders match and execute against each other.
/// It contains all the information needed to track and report the trade.
///
/// # Fields
/// * `quantity` - The size of this fill (may be partial)
/// * `price` - The price at which the fill occurred
/// * `taker_order_id` - The order that initiated the match (incoming order)
/// * `maker_order_id` - The resting order that was matched against
///
/// # Terminology
/// * Maker: The passive order already resting in the book
/// * Taker: The aggressive order that crosses the spread and initiates the trade
///
/// # Example
/// ```
/// # use rust_decimal_macros::dec;
/// # use limitbook::{OrderBook, OrderSide, Fill};
/// # fn main() {
/// let mut book = OrderBook::new(dec!(0.01)).unwrap();
///
/// // Add a resting sell order (maker)
/// let (maker_id, _) = book.add_limit_order(
///     OrderSide::Sell,
///     dec!(100.00),
///     dec!(10),
/// ).expect("invalid order");
///
/// // Add a buy order that crosses (taker)
/// let (taker_id, fills) = book.add_limit_order(
///     OrderSide::Buy,
///     dec!(100.00),
///     dec!(5),
/// ).expect("invalid order");
///
/// // Examine the fill
/// let fill = &fills[0];
/// assert_eq!(fill.quantity, dec!(5));
/// assert_eq!(fill.price, dec!(100.00));
/// assert_eq!(fill.maker_order_id, maker_id);
/// assert_eq!(fill.taker_order_id, taker_id);
/// # }
/// ```
pub struct Fill {
    pub quantity: Decimal,
    pub price: Decimal,          // The price this fill occurred at
    pub taker_order_id: OrderId, // The incoming order
    pub maker_order_id: OrderId, // The resting order it matched with
}
