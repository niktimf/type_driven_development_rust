//! Сквозной словарь домена.
//!
//! Часть того, что нужно части 2, уже есть в части 1 — переэкспортируем.
//! То, что часть 2 уточняет (всё, что несёт валюту котировки), определяем здесь.

use std::fmt;
use std::marker::PhantomData;
use std::ops::Mul;

use rust_decimal::Decimal;

pub use tdd_01_foundations::adt::CancelReason;
pub use tdd_01_foundations::newtype::market::{
    LotSize, PriceError, Quantity, QuantityError, Side, TickSize,
};
pub use tdd_01_foundations::phantom::{Eur, Id, InstrumentId, InstrumentTag, OrderTag, Usd};
pub use tdd_01_foundations::typestate::RejectReason;
pub use tdd_01_foundations::uninhabited::ClientOrderId;

use tdd_01_foundations::newtype::market::InstrumentSpec as RawSpec;

mod sealed {
    pub trait Sealed {}
}

/// Валюта. Список закрыт: реализовать снаружи нельзя — приватный супертрейт.
///
/// Часть 1 оставляла `Money<Currency>` без bound-а, и «сумма в идентификаторах
/// заявки» компилировалась. Здесь эта дыра закрыта:
///
/// ```compile_fail
/// use tdd_02_contracts::domain::Money;
/// use tdd_01_foundations::phantom::OrderId;
///
/// // the trait bound `Id<OrderTag>: Currency` is not satisfied
/// fn takes_money(_m: Money<OrderId>) {}
/// ```
pub trait Currency: sealed::Sealed {
    /// Код по ISO 4217.
    const CODE: &'static str;
    /// Сколько знаков после запятой: USD — 2, JPY — 0.
    const MINOR_UNITS: u32;
}

impl sealed::Sealed for Usd {}
impl Currency for Usd {
    const CODE: &'static str = "USD";
    const MINOR_UNITS: u32 = 2;
}

impl sealed::Sealed for Eur {}
impl Currency for Eur {
    const CODE: &'static str = "EUR";
    const MINOR_UNITS: u32 = 2;
}

/// Деньги в валюте `Quote`. В отличие от части 1, параметр ограничен.
#[derive(Debug, PartialEq, Eq)]
pub struct Money<Quote: Currency> {
    amount: Decimal,
    _quote: PhantomData<Quote>,
}

// Вручную, без bound-а `Quote: Copy`: маркер валюты физически не хранится,
// копировать в нём нечего. Тот же приём, что часть 1 применяет к `Id<Tag>`.
// С `#[derive(Copy)]` было бы `impl<Quote: Copy> Copy`, и геттеры ниже
// перестали бы компилироваться (E0507: cannot move out of `self.amount`).
impl<Quote: Currency> Clone for Money<Quote> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<Quote: Currency> Copy for Money<Quote> {}

impl<Quote: Currency> Money<Quote> {
    pub const fn new(amount: Decimal) -> Self {
        Self {
            amount,
            _quote: PhantomData,
        }
    }

    pub const fn amount(&self) -> Decimal {
        self.amount
    }
}

/// Валюта видна в выводе, хотя значения типа `Quote` не существует:
/// данные берутся из ассоциированных констант трейта.
impl<Quote: Currency> fmt::Display for Money<Quote> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Точность в форматтере, а не `round_dp`: последний округляет,
        // но не дополняет нулями — 100 так и осталось бы «100», а не «100.00».
        write!(
            f,
            "{:.*} {}",
            Quote::MINOR_UNITS as usize,
            self.amount,
            Quote::CODE
        )
    }
}

/// Цена в валюте котировки. Уточняет `Price` из части 1.
///
/// Собрать напрямую нельзя: значение приходит только из
/// [`InstrumentSpec::price`], который уже проверил его через часть 1.
#[derive(Debug, PartialEq, Eq)]
pub struct Price<Quote> {
    amount: Decimal,
    _quote: PhantomData<Quote>,
}

// Как и у `Money` — вручную, без bound-а на маркер.
impl<Quote> Clone for Price<Quote> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<Quote> Copy for Price<Quote> {}

impl<Quote> Price<Quote> {
    /// Приватный конструктор: вызывается только из [`InstrumentSpec::price`],
    /// то есть после проверки инварианта в части 1.
    const fn checked(amount: Decimal) -> Self {
        Self {
            amount,
            _quote: PhantomData,
        }
    }

    pub const fn amount(&self) -> Decimal {
        self.amount
    }
}

/// Спецификация инструмента, знающая валюту котировки.
/// Проверки делегируются части 1, валюта проставляется здесь.
///
/// Шаги хранятся отдельными полями, а не готовым `RawSpec`: у части 1 на
/// `InstrumentSpec` нет ни `Debug`, ни `Clone`, и обёртка их бы не получила.
#[derive(Debug, Clone, Copy)]
pub struct InstrumentSpec<Quote> {
    tick_size: TickSize,
    lot_size: LotSize,
    _quote: PhantomData<Quote>,
}

impl<Quote: Currency> InstrumentSpec<Quote> {
    pub const fn new(tick_size: TickSize, lot_size: LotSize) -> Self {
        Self {
            tick_size,
            lot_size,
            _quote: PhantomData,
        }
    }

    /// Проверку кратности тику делает часть 1 — здесь только проставляется валюта.
    pub fn price(&self, value: Decimal) -> Result<Price<Quote>, PriceError> {
        self.raw().price(value).map(|p| Price::checked(p.amount()))
    }

    pub fn quantity(&self, value: Decimal) -> Result<Quantity, QuantityError> {
        self.raw().quantity(value)
    }

    fn raw(&self) -> RawSpec {
        RawSpec {
            tick_size: self.tick_size,
            lot_size: self.lot_size,
        }
    }
}

/// Номинал заявки. Несущая конструкция раздела про вход и выход:
/// `Rhs` (здесь [`Quantity`]) выбирает вызывающий, `Output` фиксирует реализация.
/// Закрывает долг части 1, где `notional` возвращал голый `Decimal`.
///
/// Умножение цены на цену не запрещали отдельно — просто такого `impl` нет,
/// а значит, операции не существует:
///
/// ```compile_fail
/// use rust_decimal::Decimal;
/// use tdd_02_contracts::domain::{InstrumentSpec, LotSize, TickSize, Usd};
///
/// let spec: InstrumentSpec<Usd> = InstrumentSpec::new(
///     TickSize::new(Decimal::new(1, 2)).unwrap(),
///     LotSize::new(Decimal::ONE).unwrap(),
/// );
/// let price = spec.price(Decimal::new(18550, 2)).unwrap();
///
/// // the trait bound `Price<Usd>: Mul<Price<Usd>>` is not satisfied
/// let _nonsense = price * price;
/// ```
impl<Quote: Currency> Mul<Quantity> for Price<Quote> {
    type Output = Money<Quote>;

    fn mul(self, quantity: Quantity) -> Money<Quote> {
        Money::new(self.amount * quantity.amount())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> InstrumentSpec<Usd> {
        InstrumentSpec::new(
            TickSize::new(Decimal::new(1, 2)).unwrap(),
            LotSize::new(Decimal::ONE).unwrap(),
        )
    }

    #[test]
    fn notional_carries_the_quote_currency() {
        let spec = spec();
        let price = spec.price(Decimal::new(18550, 2)).unwrap();
        let quantity = spec.quantity(Decimal::from(10)).unwrap();

        let notional: Money<Usd> = price * quantity;
        assert_eq!(notional.amount(), Decimal::new(185500, 2));
        assert_eq!(notional.to_string(), "1855.00 USD");
    }

    #[test]
    fn same_number_different_currencies_are_different_types() {
        let usd = Money::<Usd>::new(Decimal::from(100));
        let eur = Money::<Eur>::new(Decimal::from(100));

        assert_eq!(usd.amount(), eur.amount());
        assert_eq!(usd.to_string(), "100.00 USD");
        assert_eq!(eur.to_string(), "100.00 EUR");
    }
}
