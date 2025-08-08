# High-Performance Central Limit Order Book (CLOB)

A Rust implementation of a high-performance in-memory Central Limit Order Book (CLOB) that supports limit and market orders with price-time priority matching.

## Requirements

- Rust (stable)
- Cargo

The project uses standard Rust tooling. All dependencies are managed through Cargo.

## Features

- **Order Types**: Support for both Limit and Market orders
- **Price-Time Priority**: Standard matching logic where better prices and earlier orders get priority
- **Efficient Data Structures**: 
  - `BTreeMap` for ordered price levels
  - `VecDeque` for time priority within price levels
  - `HashMap` for O(1) order lookup
- **Volume Tracking**: Maintained at both tick and book level for quick liquidity checks
- **Clean API**: Simple interface for adding and canceling orders

## Usage

```rust
use rust_decimal_macros::dec;
use orderbook::{OrderBook, OrderSide};

// Create a new order book with 0.01 tick size
let mut book = OrderBook::new(dec!(0.01));

// Add a limit sell order
let (sell_id, _) = book.add_limit_order(
    OrderSide::Sell,
    dec!(100.00),  // Price
    dec!(50),      // Quantity
);

// Add a limit buy order that crosses the book
let (buy_id, fills) = book.add_limit_order(
    OrderSide::Buy,
    dec!(100.00),  // Price matches -> will execute
    dec!(25),      // Quantity
);

// Execute a market order
let fills = book.execute_market_order(
    OrderSide::Buy,
    dec!(25),
).unwrap();

// Cancel an order
book.cancel_limit_order(sell_id)?;
```

## Design Decisions

### Data Structure Choice

1. **Price Level Organization**: `BTreeMap<Tick, Orders>`
   - Ordered price levels for efficient best price access
   - O(log n) insertion and lookup
   - Natural ordering for bid/ask matching

2. **Order Storage**: `VecDeque<Order>`
   - FIFO queue for time priority
   - O(1) push/pop for order management
   - Efficient iteration for matching

3. **Order Lookup**: `HashMap<OrderId, (OrderSide, Tick)>`
   - O(1) access for order cancellation
   - Maintains side and price level information
   - Enables quick order location without tree traversal

### Performance Considerations

- Tick-level volume caching to avoid recalculation
- Order count tracking for efficient empty level cleanup
- Minimal data copying and cloning
- Efficient use of Rust's ownership system

### Memory vs Speed Trade-offs

- Additional memory used for order lookup table
- Volume caching at multiple levels
- Trade-off favors operation speed over memory efficiency

## Implementation Details

### Price-Time Priority

The matching logic follows standard price-time priority:
1. Better prices get matched first
   - For bids: Higher prices have priority
   - For asks: Lower prices have priority
2. Within each price level, older orders are matched first

### Order Matching Process

1. **Limit Orders**:
   - Check for crosses against opposite side
   - Match at best available prices
   - Remaining quantity added to book

2. **Market Orders**:
   - Quick liquidity check
   - Match against best prices until filled
   - Fail if insufficient liquidity

## Development

### Building and Testing

```bash
# Build the project
cargo build

# Run the test suite
cargo test

# Run benchmarks
cargo bench
```

### Testing Coverage

Comprehensive test suite covering:
- Basic order addition and cancellation
- Price-time priority matching
- Partial fills and order cleanup
- Edge cases and error conditions

## Benchmarks

This section describes the benchmarks and performance characteristics of the order book implementation.

### Benchmark Scenarios

1. **Limit Order (No Cross)**
   - Adding a limit order that doesn't cross the book
   - Tests basic order insertion performance

2. **Limit Order (With Cross)**
   - Adding a limit order that crosses the book
   - Tests matching engine performance

3. **Market Order**
   - Executing market orders
   - Tests immediate execution performance

4. **Cancel Order**
   - Canceling existing orders
   - Tests order lookup and removal performance

### Test Setup
- Book depth: 100 price levels
- Orders per level: 10
- Total orders: 2000 (1000 each side)
- Tick size: 0.01

### Performance Considerations

1. Data Structure Choice
   - BTreeMap for price levels: O(log n)
   - VecDeque for order queues: O(1)
   - HashMap for order lookup: O(1)

2. Memory vs Speed Trade-offs
   - Additional memory for order lookup
   - Cached volumes for quick access
   - Price level organization for efficient matching

### Benchmark Results

Our order book implementation achieves excellent performance:

- **Limit Orders**: 3-5 million orders/second
  - Non-crossing: ~204ns (184-243ns range)
  - With crossing/matching: ~290ns (213-423ns range)
  - Matching adds some overhead as expected

- **Market Orders**: ~31ns (~30 million/second)
  - Extremely consistent performance
  - Fast due to immediate execution path
  - Benefits from cached volume tracking

- **Cancellations**: ~31ns (~30 million/second)
  - O(1) lookup strategy pays off
  - Very consistent timing
  - Efficient cleanup of price levels

These results demonstrate that our choice of data structures (BTreeMap for price levels, VecDeque for order queues, HashMap for lookups) provides an excellent balance of functionality and performance. The implementation can handle high-frequency trading scenarios while maintaining clean, safe Rust code.

Note: Benchmarks run on a standard development machine. Real-world performance may vary based on market conditions, order book depth, and system load.
