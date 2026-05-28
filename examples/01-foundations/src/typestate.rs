//! Раздел статьи «Typestate» — состояние объекта закодировано в его типе.
//!
//! Жизненный цикл заявки на бирже:
//!
//! ```text
//! DraftOrder --submit--> WorkingOrder --fill----> FilledOrder
//!                            |
//!                            \---cancel--------> CancelledOrder
//! ```
//!
//! Каждый переход потребляет `self`, и старое состояние после перехода физически
//! уничтожено. Вызвать метод не того состояния не получится — его нет в `impl`-блоке.
//! В частности, у `CancelledOrder` нет методов, чтобы «висеть в стакане»: отменённая
//! заявка не может снова стать активной.

use crate::newtype::ids::OrderId;
use crate::newtype::market::{InstrumentId, Price, Quantity, Side};

/// Биржа отклонила заявку при постановке.
#[derive(Debug, PartialEq, Eq)]
pub enum RejectReason {
    MarketClosed,
    InsufficientFunds,
}

/// Черновик заявки — ещё не отправлен на биржу, биржевого идентификатора нет.
///
/// Compile-fail: метод не своего состояния не вызвать.
///
/// ```compile_fail
/// use tdd_01_foundations::typestate::DraftOrder;
///
/// let order = DraftOrder::new(/* ... */);
/// order.cancel(); // no method `cancel` on DraftOrder — отменять нечего, ещё не подан
/// ```
///
/// ```compile_fail
/// use tdd_01_foundations::typestate::DraftOrder;
///
/// let order = DraftOrder::new(/* ... */);
/// let _first = order.submit();
/// let _second = order.submit(); // order moved — повторно подать нельзя
/// ```
pub struct DraftOrder {
    instrument: InstrumentId,
    side: Side,
    price: Price,
    quantity: Quantity,
}

impl DraftOrder {
    pub fn new(instrument: InstrumentId, side: Side, price: Price, quantity: Quantity) -> Self {
        Self {
            instrument,
            side,
            price,
            quantity,
        }
    }

    /// Постановка в стакан. Биржа присваивает идентификатор и может отклонить заявку.
    pub fn submit(self) -> Result<WorkingOrder, RejectReason> {
        // В реальном коде — поход на биржу. Здесь упрощённо: всегда принимаем
        // и выдаём фиксированный id.
        Ok(WorkingOrder {
            id: OrderId(1),
            instrument: self.instrument,
            side: self.side,
            price: self.price,
            quantity: self.quantity,
        })
    }
}

/// Активная заявка «в стакане». Уже есть биржевой идентификатор.
pub struct WorkingOrder {
    id: OrderId,
    instrument: InstrumentId,
    side: Side,
    price: Price,
    quantity: Quantity,
}

impl WorkingOrder {
    pub fn id(&self) -> OrderId {
        self.id
    }

    pub fn instrument(&self) -> InstrumentId {
        self.instrument
    }

    pub fn side(&self) -> Side {
        self.side
    }

    /// Полное исполнение — переход в терминальное состояние.
    pub fn fill(self) -> FilledOrder {
        FilledOrder {
            id: self.id,
            price: self.price,
            quantity: self.quantity,
        }
    }

    /// Отмена — другое терминальное состояние.
    pub fn cancel(self) -> CancelledOrder {
        CancelledOrder { id: self.id }
    }
}

/// Исполненная заявка. Терминальное состояние: переходов из него нет.
pub struct FilledOrder {
    id: OrderId,
    price: Price,
    quantity: Quantity,
}

impl FilledOrder {
    pub fn id(&self) -> OrderId {
        self.id
    }

    pub fn price(&self) -> Price {
        self.price
    }

    pub fn quantity(&self) -> Quantity {
        self.quantity
    }
}

/// Отменённая заявка. Терминальное состояние: методов «снова стать активной» нет.
pub struct CancelledOrder {
    id: OrderId,
}

impl CancelledOrder {
    pub fn id(&self) -> OrderId {
        self.id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::newtype::market::{InstrumentSpec, LotSize, TickSize};
    use rust_decimal::dec;

    fn draft() -> DraftOrder {
        let spec = InstrumentSpec {
            tick_size: TickSize::new(dec!(0.01)).unwrap(),
            lot_size: LotSize::new(dec!(1)).unwrap(),
        };
        DraftOrder::new(
            InstrumentId(1),
            Side::Buy,
            spec.price(dec!(185.50)).unwrap(),
            spec.quantity(dec!(10)).unwrap(),
        )
    }

    #[test]
    fn draft_submits_to_working() {
        let working = draft().submit().expect("accepted");
        assert_eq!(working.id(), OrderId(1));
    }

    #[test]
    fn working_order_fills() {
        let filled = draft().submit().unwrap().fill();
        assert_eq!(filled.id(), OrderId(1));
        assert_eq!(filled.price().amount(), dec!(185.50));
        assert_eq!(filled.quantity().amount(), dec!(10));
    }

    #[test]
    fn working_order_cancels() {
        let cancelled = draft().submit().unwrap().cancel();
        assert_eq!(cancelled.id(), OrderId(1));
    }
}
