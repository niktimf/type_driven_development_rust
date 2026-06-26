//! Раздел статьи «Phantom types» — параметры типа без рантайм-представления.
//!
//! Две демонстрации на биржевом домене:
//! - [`Id<Tag>`] — одна generic-структура, много типов-маркеров. В памяти
//!   `Id<OrderTag>` и `Id<InstrumentTag>` — один и тот же `u64`, но компилятор их различает.
//! - [`Money<Currency>`] — валюта как phantom-тег: сложить `Money<Usd>` и `Money<Eur>`
//!   компилятор не даст, хотя внутри обоих один `Decimal`.

use std::marker::PhantomData;

use rust_decimal::Decimal;

use crate::newtype::market::{Price, Quantity};

/// Generic-идентификатор с phantom-тегом. Все `Id<Tag>` в рантайме — это
/// 8 байт `u64`; разница между маркерами существует только на этапе компиляции.
///
/// Пример из статьи: `InstrumentId` нельзя передать в функцию, ожидающую `OrderId`.
///
/// ```compile_fail
/// use tdd_01_foundations::phantom::{Id, OrderId, InstrumentId};
///
/// fn cancel_order(_id: OrderId) {}
///
/// let instrument: InstrumentId = Id::new(42);
/// cancel_order(instrument); // expected `Id<OrderTag>`, found `Id<InstrumentTag>`
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Id<Tag> {
    raw: u64,
    _tag: PhantomData<Tag>,
}

impl<Tag> Id<Tag> {
    pub const fn new(raw: u64) -> Self {
        Self {
            raw,
            _tag: PhantomData,
        }
    }

    pub const fn raw(&self) -> u64 {
        self.raw
    }
}

/// Маркер заказа. Derive-ы навешены, чтобы `#[derive(...)]` на `Id<Tag>`
/// работал — он добавляет bound `Tag: Debug + Clone + ...` на сгенерированный impl.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OrderTag;

/// Маркер инструмента — аналогично.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InstrumentTag;

pub type OrderId = Id<OrderTag>;
pub type InstrumentId = Id<InstrumentTag>;

/// Валюта как phantom-тег. Внутри — `Decimal` (десятичные деньги из newtype-раздела),
/// но `Money<Usd>` и `Money<Eur>` — разные типы, и сложить их компилятор не даст.
///
/// ```compile_fail
/// use tdd_01_foundations::phantom::{Money, Usd, Eur};
/// use rust_decimal::dec;
///
/// let usd = Money::<Usd>::new(dec!(100));
/// let eur = Money::<Eur>::new(dec!(100));
/// let _ = usd + eur; // cannot add `Money<Eur>` to `Money<Usd>`
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Money<Currency> {
    amount: Decimal,
    _currency: PhantomData<Currency>,
}

impl<Currency> Money<Currency> {
    pub const fn new(amount: Decimal) -> Self {
        Self {
            amount,
            _currency: PhantomData,
        }
    }

    pub const fn amount(&self) -> Decimal {
        self.amount
    }
}

/// Сложить можно только деньги одной валюты: оба операнда — `Money<Currency>` с
/// одним и тем же `Currency`. `Money<Usd> + Money<Eur>` не компилируется.
impl<Currency> std::ops::Add for Money<Currency> {
    type Output = Money<Currency>;

    fn add(self, rhs: Money<Currency>) -> Money<Currency> {
        Money::new(self.amount + rhs.amount)
    }
}

/// Номинал заявки в валюте инструмента: тот самый `price × quantity` из
/// newtype-раздела ([`crate::newtype::market::notional`]), но обёрнутый в `Money<C>`.
/// Валюту `C` задаёт спецификация инструмента, так что номиналы в разных валютах
/// сложить уже не получится — та же защита, что у `Money<Usd> + Money<Eur>`.
pub fn notional<C>(price: Price, quantity: Quantity) -> Money<C> {
    Money::new(price.amount() * quantity.amount())
}

/// Доллар США.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Usd;

/// Евро.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Eur;

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::dec;
    use std::mem::size_of;

    #[test]
    fn phantom_does_not_add_runtime_overhead() {
        // PhantomData<Tag> в рантайме отсутствует — размер равен u64.
        assert_eq!(size_of::<Id<OrderTag>>(), size_of::<u64>());
        assert_eq!(size_of::<Id<InstrumentTag>>(), size_of::<u64>());
        assert_eq!(size_of::<OrderId>(), 8);
    }

    #[test]
    fn order_id_and_instrument_id_carry_the_same_payload() {
        let order: OrderId = Id::new(42);
        let instrument: InstrumentId = Id::new(42);
        // payload одинаковый, типы — разные (что и проверяет compile_fail-доктест).
        assert_eq!(order.raw(), instrument.raw());
    }

    #[test]
    fn generic_method_works_for_all_tags() {
        // Один impl<Tag> Id<Tag> обслуживает оба варианта без дублирования.
        let order = OrderId::new(1);
        let instrument = InstrumentId::new(2);
        assert_eq!(order.raw(), 1);
        assert_eq!(instrument.raw(), 2);
    }

    #[test]
    fn same_currency_money_adds() {
        let a = Money::<Usd>::new(dec!(100.50));
        let b = Money::<Usd>::new(dec!(0.25));
        assert_eq!((a + b).amount(), dec!(100.75));
    }

    #[test]
    fn money_tag_does_not_add_overhead() {
        // Внутри только Decimal — phantom-валюта ничего не весит.
        assert_eq!(size_of::<Money<Usd>>(), size_of::<Decimal>());
    }

    #[test]
    fn notional_is_money_in_instrument_currency() {
        use crate::newtype::market::{InstrumentSpec, LotSize, TickSize};
        let spec = InstrumentSpec {
            tick_size: TickSize::new(dec!(0.01)).unwrap(),
            lot_size: LotSize::new(dec!(1)).unwrap(),
        };
        let price = spec.price(dec!(185.50)).unwrap();
        let quantity = spec.quantity(dec!(10)).unwrap();
        // Стоимость сделки — Money в валюте инструмента (здесь Usd), а не голый Decimal.
        let n: Money<Usd> = notional(price, quantity);
        assert_eq!(n.amount(), dec!(1855.00));
    }
}
