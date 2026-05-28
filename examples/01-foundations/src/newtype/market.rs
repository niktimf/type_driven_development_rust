//! Раздел статьи «Newtype» — биржевой пример: цена и объём с инвариантом.
//!
//! - [`Price`] / [`Quantity`] — newtype над `Decimal` с приватным полем.
//! - [`TickSize`] / [`LotSize`] — шаги инструмента, тоже newtype (иначе их легко
//!   перепутать местами в [`InstrumentSpec`]).
//! - [`InstrumentSpec`] — где живёт smart constructor: инвариант цены зависит от
//!   инструмента (его шага цены), поэтому конструктор — на спецификации, а не на
//!   самом `Price`. Это усложнённый случай; классический smart constructor живёт
//!   на самом типе, когда инвариант самодостаточен.
//! - [`Side`] / [`Order`] — собранная заявка (product type из value objects).

use rust_decimal::Decimal;

use crate::newtype::ids::OrderId;

/// Идентификатор инструмента. Простой newtype без инварианта.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InstrumentId(pub u64);

/// Цена. Приватное поле — в обход [`InstrumentSpec::price`] не собрать.
///
/// Перепутать `Price` и [`Quantity`] компилятор не даст, хотя внутри обоих `Decimal`:
///
/// ```compile_fail
/// use tdd_01_foundations::newtype::market::{InstrumentSpec, Price, Quantity};
///
/// fn takes_price(_p: Price) {}
///
/// let q: Quantity = todo!();
/// takes_price(q); // expected `Price`, found `Quantity`
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Price(Decimal);

/// Объём. Устроен зеркально [`Price`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Quantity(Decimal);

/// Шаг цены (tick size). Тоже newtype, иначе в [`InstrumentSpec`] его легко
/// перепутать с [`LotSize`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TickSize(Decimal);

/// Шаг объёма (lot size).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LotSize(Decimal);

/// Сторона заявки. Заменяет неоднозначный `is_buy: bool` из «Проблемы».
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PriceError {
    NonPositive,
    NotOnTick { tick: Decimal },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuantityError {
    NonPositive,
    NotOnLot { lot: Decimal },
}

impl TickSize {
    pub fn new(step: Decimal) -> Result<Self, PriceError> {
        if step <= Decimal::ZERO {
            return Err(PriceError::NonPositive);
        }
        Ok(Self(step))
    }

    pub fn amount(&self) -> Decimal {
        self.0
    }
}

impl LotSize {
    pub fn new(step: Decimal) -> Result<Self, QuantityError> {
        if step <= Decimal::ZERO {
            return Err(QuantityError::NonPositive);
        }
        Ok(Self(step))
    }

    pub fn amount(&self) -> Decimal {
        self.0
    }
}

/// Спецификация инструмента: здесь живёт smart constructor цены и объёма, потому
/// что инвариант (кратность шагу) зависит от инструмента, а не от самого числа.
pub struct InstrumentSpec {
    pub tick_size: TickSize,
    pub lot_size: LotSize,
    // ...min/max, валюта котировки
}

impl InstrumentSpec {
    /// Строгий конструктор: некратная цена — ошибка, а не повод округлить молча.
    pub fn price(&self, value: Decimal) -> Result<Price, PriceError> {
        let tick = self.tick_size.amount();
        if value <= Decimal::ZERO {
            return Err(PriceError::NonPositive);
        }
        if value % tick != Decimal::ZERO {
            return Err(PriceError::NotOnTick { tick });
        }
        Ok(Price(value))
    }

    /// Зеркально [`InstrumentSpec::price`], только проверка идёт против шага лота.
    pub fn quantity(&self, value: Decimal) -> Result<Quantity, QuantityError> {
        let lot = self.lot_size.amount();
        if value <= Decimal::ZERO {
            return Err(QuantityError::NonPositive);
        }
        if value % lot != Decimal::ZERO {
            return Err(QuantityError::NotOnLot { lot });
        }
        Ok(Quantity(value))
    }
}

impl Price {
    /// Доступ к значению через явный геттер, а не `Deref`/`AsRef`.
    pub fn amount(&self) -> Decimal {
        self.0
    }
}

impl Quantity {
    pub fn amount(&self) -> Decimal {
        self.0
    }
}

/// Заявка: product type, собранный из типизированных value objects.
pub struct Order {
    pub instrument: InstrumentId,
    pub side: Side,
    pub price: Price,
    pub quantity: Quantity,
}

/// Номинал заявки = цена × объём. Умножение `Decimal` точное.
pub fn notional(price: Price, quantity: Quantity) -> Decimal {
    price.amount() * quantity.amount()
}

/// Сигнатура из статьи: благодаря типам аргументы не перепутать местами.
pub fn place_limit_order(
    symbol: &str,
    side: Side,
    price: Price,
    quantity: Quantity,
) -> OrderId {
    let _ = (symbol, side, price, quantity);
    OrderId(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::dec;

    fn spec() -> InstrumentSpec {
        InstrumentSpec {
            tick_size: TickSize::new(dec!(0.01)).unwrap(),
            lot_size: LotSize::new(dec!(1)).unwrap(),
        }
    }

    #[test]
    fn on_tick_price_is_accepted() {
        let p = spec().price(dec!(185.50)).unwrap();
        assert_eq!(p.amount(), dec!(185.50));
    }

    #[test]
    fn off_tick_price_is_rejected() {
        assert_eq!(
            spec().price(dec!(185.505)),
            Err(PriceError::NotOnTick { tick: dec!(0.01) })
        );
    }

    #[test]
    fn non_positive_price_is_rejected() {
        assert_eq!(spec().price(dec!(0)), Err(PriceError::NonPositive));
        assert_eq!(spec().price(dec!(-5)), Err(PriceError::NonPositive));
    }

    #[test]
    fn quantity_checks_against_lot() {
        assert_eq!(spec().quantity(dec!(10)).unwrap().amount(), dec!(10));
        assert_eq!(
            spec().quantity(dec!(1.5)),
            Err(QuantityError::NotOnLot { lot: dec!(1) })
        );
    }

    #[test]
    fn decimal_addition_is_exact() {
        // Ровно тот случай, на котором врёт f64: 0.1 + 0.2 == 0.3.
        assert_eq!(dec!(0.1) + dec!(0.2), dec!(0.3));
    }

    #[test]
    fn notional_is_price_times_quantity() {
        let s = spec();
        let order = Order {
            instrument: InstrumentId(1),
            side: Side::Buy,
            price: s.price(dec!(185.50)).unwrap(),
            quantity: s.quantity(dec!(10)).unwrap(),
        };
        assert_eq!(notional(order.price, order.quantity), dec!(1855.00));
    }
}
