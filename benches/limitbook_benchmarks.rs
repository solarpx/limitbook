use criterion::{black_box, criterion_group, criterion_main, Criterion};
use limitbook::{OrderBook, OrderSide};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

fn setup_book_with_depth(depth: u32, orders_per_level: u32) -> OrderBook {
    let mut book = OrderBook::new(dec!(0.01)).expect("tick spacing must be positive");

    // Add asks starting at 100.00
    for i in 0..depth {
        for _ in 0..orders_per_level {
            book.add_limit_order(
                OrderSide::Sell,
                dec!(100.00) + Decimal::from(i) * dec!(0.01),
                dec!(1.0),
            )
            .expect("invalid order");
        }
    }

    // Add bids starting at 99.99
    for i in 0..depth {
        for _ in 0..orders_per_level {
            book.add_limit_order(
                OrderSide::Buy,
                dec!(99.99) - Decimal::from(i) * dec!(0.01),
                dec!(1.0),
            )
            .expect("invalid order");
        }
    }

    book
}

fn benchmark_limit_order_no_cross(c: &mut Criterion) {
    let mut book = setup_book_with_depth(100, 10); // 1000 orders on each side

    c.bench_function("add_limit_order_no_cross", |b| {
        b.iter(|| {
            book.add_limit_order(
                black_box(OrderSide::Buy),
                black_box(dec!(99.98)),
                black_box(dec!(1.0)),
            )
            .expect("invalid order")
        })
    });
}

fn benchmark_limit_order_with_cross(c: &mut Criterion) {
    let mut book = setup_book_with_depth(100, 10);

    c.bench_function("add_limit_order_with_cross", |b| {
        b.iter(|| {
            book.add_limit_order(
                black_box(OrderSide::Buy),
                black_box(dec!(100.02)), // Will cross
                black_box(dec!(1.0)),
            )
            .expect("invalid order")
        })
    });
}

fn benchmark_market_order(c: &mut Criterion) {
    let mut book = setup_book_with_depth(100, 10);

    c.bench_function("execute_market_order", |b| {
        b.iter(|| book.execute_market_order(black_box(OrderSide::Buy), black_box(dec!(5.0))))
    });
}

fn benchmark_cancel_order(c: &mut Criterion) {
    let mut book = setup_book_with_depth(100, 10);
    let (order_id, _) = book
        .add_limit_order(OrderSide::Buy, dec!(99.98), dec!(1.0))
        .expect("invalid order");

    c.bench_function("cancel_limit_order", |b| {
        b.iter(|| book.cancel_limit_order(black_box(order_id)))
    });
}

criterion_group!(
    benches,
    benchmark_limit_order_no_cross,
    benchmark_limit_order_with_cross,
    benchmark_market_order,
    benchmark_cancel_order
);
criterion_main!(benches);
