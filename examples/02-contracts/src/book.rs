//! Снимок стакана фиксированной глубины. Уровни — те же [`Level`],
//! что отдаёт [`crate::feed::MarketDataFeed`]; теперь их количество в типе.

use rust_decimal::Decimal;

use crate::domain::Currency;
use crate::feed::Level;

mod sealed {
    pub trait Sealed {}
}

/// Носитель числа: с ним «допустимая глубина» выражается обычным trait bound.
pub struct BookDepth<const N: usize>;

/// Список глубин задаёт не площадка, а наш код: под каждую написана стратегия,
/// произвольной глубине взяться неоткуда — запечатано.
pub trait SupportedDepth: sealed::Sealed {}

impl sealed::Sealed for BookDepth<1> {}
impl SupportedDepth for BookDepth<1> {}
impl sealed::Sealed for BookDepth<5> {}
impl SupportedDepth for BookDepth<5> {}
impl sealed::Sealed for BookDepth<10> {}
impl SupportedDepth for BookDepth<10> {}

/// Стакан на `DEPTH` уровней в валюте `Quote`.
///
/// `OrderBook<Usd, 5>` и `OrderBook<Usd, 10>` — разные типы:
///
/// ```compile_fail
/// use tdd_02_contracts::book::OrderBook;
/// use tdd_02_contracts::domain::Usd;
///
/// fn needs_ten(_book: OrderBook<Usd, 10>) {}
///
/// let five: OrderBook<Usd, 5> = todo!();
/// needs_ten(five); // expected `OrderBook<Usd, 10>`, found `OrderBook<Usd, 5>`
/// ```
///
/// Стакана нулевой глубины не существует — `spread` безопасен по построению:
///
/// ```compile_fail
/// use tdd_02_contracts::book::OrderBook;
/// use tdd_02_contracts::domain::Usd;
///
/// let empty = OrderBook::<Usd, 0>::new([], []);
/// // the trait bound `BookDepth<0>: SupportedDepth` is not satisfied
/// ```
#[derive(Debug, PartialEq, Eq)]
pub struct OrderBook<Quote, const DEPTH: usize> {
    bids: [Level<Quote>; DEPTH],
    asks: [Level<Quote>; DEPTH],
}

// Без bound-а на `Quote` — тот же приём, что у `Level` и `Price`:
// маркер валюты физически не хранится, `#[derive(Copy)]` дал бы лишний bound.
impl<Quote, const DEPTH: usize> Clone for OrderBook<Quote, DEPTH> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<Quote, const DEPTH: usize> Copy for OrderBook<Quote, DEPTH> {}

impl<Quote: Currency, const DEPTH: usize> OrderBook<Quote, DEPTH>
where
    BookDepth<DEPTH>: SupportedDepth,
{
    pub const fn new(bids: [Level<Quote>; DEPTH], asks: [Level<Quote>; DEPTH]) -> Self {
        Self { bids, asks }
    }
}

impl<Quote: Currency, const DEPTH: usize> OrderBook<Quote, DEPTH> {
    pub const fn depth(&self) -> usize {
        DEPTH
    }

    /// Уровень хотя бы один — гарантировано отсутствием конструктора для `DEPTH = 0`.
    pub fn spread(&self) -> Decimal {
        self.asks[0].price().amount() - self.bids[0].price().amount()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Usd;
    use crate::feed::Level;
    use crate::order::tests_support::spec;
    use rust_decimal::Decimal;

    fn level(price: i64) -> Level<Usd> {
        let spec = spec();
        Level::new(
            spec.price(Decimal::new(price, 2)).unwrap(),
            spec.quantity(Decimal::from(10)).unwrap(),
        )
    }

    #[test]
    fn depth_is_known_at_compile_time() {
        let book: OrderBook<Usd, 1> = OrderBook::new([level(18550)], [level(18551)]);
        assert_eq!(book.depth(), 1);
        assert_eq!(book.spread(), Decimal::new(1, 2));
    }

    #[test]
    fn book_has_no_heap_allocation() {
        // два массива по DEPTH уровней лежат прямо в структуре.
        // Сравниваем с массивами, а не с size_of::<Level>() * 20:
        // так тест не развалится, если у Level появится padding.
        assert_eq!(
            std::mem::size_of::<OrderBook<Usd, 10>>(),
            std::mem::size_of::<[Level<Usd>; 10]>() * 2
        );
    }
}
