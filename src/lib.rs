// in lib.rs
pub mod order;
pub mod order_book;
pub mod ticks;

// Re-export main types for easier use
pub use order::{Fill, Order, OrderId, OrderSide, OrderType};
pub use order_book::OrderBook;
pub use ticks::Tick;
