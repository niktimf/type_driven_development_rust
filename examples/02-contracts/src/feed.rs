//! Поток котировок. Уровень стакана называется [`Level`]:
//! имя `Quote` занято валютой котировки в `Price<Quote>`.

use crate::domain::{Currency, Price, Quantity};

/// Уровень стакана: цена и объём на ней.
#[derive(Debug, PartialEq, Eq)]
pub struct Level<Quote> {
    price: Price<Quote>,
    quantity: Quantity,
}

// Без bound-а на `Quote` — иначе геттер `price()` не соберётся:
// `#[derive(Copy)]` дал бы `impl<Quote: Copy> Copy`, а в блоке
// `impl<Quote> Level<Quote>` такого bound-а нет, и E0507.
impl<Quote> Clone for Level<Quote> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<Quote> Copy for Level<Quote> {}

impl<Quote> Level<Quote> {
    pub const fn new(price: Price<Quote>, quantity: Quantity) -> Self {
        Self { price, quantity }
    }

    pub const fn price(&self) -> Price<Quote> {
        self.price
    }

    pub const fn quantity(&self) -> Quantity {
        self.quantity
    }
}

/// Стакан живёт в буфере, который перезаписывается новыми снимками.
/// Отдать итератор наружу, ничего не копируя, позволяет GAT:
/// у ассоциированного типа появляется собственное время жизни.
pub trait MarketDataFeed {
    type Quote: Currency;

    type Levels<'a>: Iterator<Item = &'a Level<Self::Quote>>
    where
        Self: 'a;

    fn bids(&self) -> Self::Levels<'_>;
    fn asks(&self) -> Self::Levels<'_>;
}

#[derive(Debug)]
pub struct BookFeed<Quote> {
    bids: Vec<Level<Quote>>,
    asks: Vec<Level<Quote>>,
}

impl<Quote> Default for BookFeed<Quote> {
    fn default() -> Self {
        Self {
            bids: Vec::new(),
            asks: Vec::new(),
        }
    }
}

impl<Quote> BookFeed<Quote> {
    pub fn push_bid(&mut self, level: Level<Quote>) {
        self.bids.push(level);
    }

    pub fn push_ask(&mut self, level: Level<Quote>) {
        self.asks.push(level);
    }
}

impl<Quote: Currency> MarketDataFeed for BookFeed<Quote> {
    type Quote = Quote;

    type Levels<'a>
        = std::slice::Iter<'a, Level<Quote>>
    where
        Self: 'a;

    fn bids(&self) -> Self::Levels<'_> {
        self.bids.iter()
    }

    fn asks(&self) -> Self::Levels<'_> {
        self.asks.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Usd;
    use crate::order::tests_support::spec;
    use rust_decimal::Decimal;

    #[test]
    fn feed_borrows_its_buffer() {
        let spec = spec();
        let mut feed: BookFeed<Usd> = BookFeed::default();
        feed.push_bid(Level::new(
            spec.price(Decimal::new(18550, 2)).unwrap(),
            spec.quantity(Decimal::from(10)).unwrap(),
        ));

        let first = feed.bids().next().unwrap();
        // Одна вставленная заявка — один уровень на выходе итератора.
        assert_eq!(first.price().amount(), Decimal::new(18550, 2));
        assert_eq!(feed.bids().count(), 1);
    }

    #[test]
    fn iterator_yields_references_into_the_buffer_not_copies() {
        // `assert_eq!` на значении не отличил бы заимствование от копии:
        // `Level` — `Copy`, и у копии те же поля. Честная проверка — адрес:
        // если итератор не копирует буфер, элемент, который он отдаёт,
        // физически лежит по тому же адресу, что и элемент в самом `Vec`.
        let spec = spec();
        let mut feed: BookFeed<Usd> = BookFeed::default();
        feed.push_bid(Level::new(
            spec.price(Decimal::new(18550, 2)).unwrap(),
            spec.quantity(Decimal::from(10)).unwrap(),
        ));

        let from_iterator: &Level<Usd> = feed.bids().next().unwrap();
        let from_buffer: &Level<Usd> = &feed.bids[0];
        assert!(std::ptr::eq(from_iterator, from_buffer));
    }
}
