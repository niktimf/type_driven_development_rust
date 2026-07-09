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
//! Каждый переход потребляет `self` — старого значения после перехода больше нет.
//! Вызвать метод не того состояния не выйдет — его нет в `impl`-блоке.
//! В частности, у `CancelledOrder` нет методов, чтобы «висеть в стакане»: отменённая
//! заявка не может снова стать активной.
//!
//! Дизайн повторяет статью: черновик собран у клиента по символу (`symbol: String`),
//! тип и цена живут в [`OrderType`], а биржа резолвит символ в [`InstrumentId`] и
//! присваивает [`OrderId`] на `submit`. Цена исполнения (`fill_price`) приходит от
//! биржи на `fill` и может отличаться от лимитной.
//!
//! В конце файла — phantom-вариант той же машины: одна структура [`Order`] на все
//! состояния после постановки, маркеры собраны в модуль [`order_state`], а черновик
//! остаётся отдельной структурой.

use std::marker::PhantomData;

use crate::adt::OrderType;
use crate::newtype::ids::OrderId;
use crate::newtype::market::{InstrumentId, Price, Quantity, Side};

/// Биржа отклонила заявку при постановке.
#[derive(Debug, PartialEq, Eq)]
pub enum RejectReason {
    MarketClosed,
    InsufficientFunds,
}

/// Черновик заявки: собран у клиента, на биржу ещё не отправлен — id заявки не
/// присвоен, символ не резолвлен в id инструмента.
/// Символ хранится строкой (`"AAPL"`), тип и цена — в [`OrderType`] (как в ADT-разделе).
///
/// Compile-fail: метод не своего состояния на черновике не вызвать.
///
/// ```compile_fail
/// # use tdd_01_foundations::typestate::DraftOrder;
/// # use tdd_01_foundations::newtype::market::{InstrumentSpec, TickSize, LotSize, Side};
/// # use rust_decimal::dec;
/// # let spec = InstrumentSpec {
/// #     tick_size: TickSize::new(dec!(0.01)).unwrap(),
/// #     lot_size: LotSize::new(dec!(1)).unwrap(),
/// # };
/// let draft = DraftOrder::limit(
///     "AAPL".to_string(),
///     Side::Buy,
///     spec.price(dec!(185.50)).unwrap(),
///     spec.quantity(dec!(10)).unwrap(),
/// );
/// draft.cancel(); // no method `cancel` on DraftOrder — отменять нечего, ещё не подан
/// ```
///
/// Compile-fail: после `submit` черновик `moved` — повторно не подать.
///
/// ```compile_fail
/// # use tdd_01_foundations::typestate::DraftOrder;
/// # use tdd_01_foundations::newtype::market::{InstrumentSpec, TickSize, LotSize, Side};
/// # use rust_decimal::dec;
/// # let spec = InstrumentSpec {
/// #     tick_size: TickSize::new(dec!(0.01)).unwrap(),
/// #     lot_size: LotSize::new(dec!(1)).unwrap(),
/// # };
/// let draft = DraftOrder::limit(
///     "AAPL".to_string(),
///     Side::Buy,
///     spec.price(dec!(185.50)).unwrap(),
///     spec.quantity(dec!(10)).unwrap(),
/// );
/// let _first = draft.submit();
/// let _second = draft.submit(); // draft moved — повторно подать нельзя
/// ```
pub struct DraftOrder {
    symbol: String,
    side: Side,
    quantity: Quantity,
    order_type: OrderType,
}

impl DraftOrder {
    /// Типизированный конструктор под лимитную заявку — тип в имени, как у
    /// `place_*`-функций. Цена идёт перед объёмом, как в [`place_limit_order`].
    ///
    /// [`place_limit_order`]: crate::newtype::market::place_limit_order
    pub fn limit(symbol: String, side: Side, price: Price, quantity: Quantity) -> Self {
        Self {
            symbol,
            side,
            quantity,
            order_type: OrderType::Limit(price),
        }
    }

    /// Рыночная заявка — цены нет в принципе.
    pub fn market(symbol: String, side: Side, quantity: Quantity) -> Self {
        Self {
            symbol,
            side,
            quantity,
            order_type: OrderType::Market,
        }
    }

    /// Стоп-лимитная заявка — стоп-триггер и лимитная цена.
    pub fn stop_limit(
        symbol: String,
        side: Side,
        stop: Price,
        limit: Price,
        quantity: Quantity,
    ) -> Self {
        Self {
            symbol,
            side,
            quantity,
            order_type: OrderType::StopLimit { stop, limit },
        }
    }

    /// Постановка в стакан: биржа резолвит символ в [`InstrumentId`],
    /// присваивает id заявки и может отклонить.
    /// Здесь упрощённо: всегда принимаем.
    pub fn submit(self) -> Result<WorkingOrder, RejectReason> {
        Ok(WorkingOrder {
            id: assign_id(),
            instrument: resolve(&self.symbol),
            side: self.side,
            quantity: self.quantity,
            order_type: self.order_type,
        })
    }
}

/// Активная заявка «в стакане»: биржа приняла её,
/// присвоила id заявки и резолвила символ в id инструмента.
/// Ждёт исполнения или отмены.
pub struct WorkingOrder {
    id: OrderId,
    instrument: InstrumentId,
    side: Side,
    quantity: Quantity,
    order_type: OrderType,
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

    pub fn order_type(&self) -> OrderType {
        self.order_type
    }

    /// Исполнение: биржа сообщает цену сделки (`fill_price`),
    /// она приходит извне и может отличаться от лимитной.
    /// Переход в терминальное состояние.
    pub fn fill(self, fill_price: Price) -> FilledOrder {
        FilledOrder {
            id: self.id,
            fill_price,
            quantity: self.quantity,
        }
    }

    /// Отмена — другое терминальное состояние.
    pub fn cancel(self) -> CancelledOrder {
        CancelledOrder { id: self.id }
    }
}

/// Исполненная заявка.
/// Терминальное состояние: переходов из него нет.
/// Цена — `fill_price` от биржи, а не из заявки.
pub struct FilledOrder {
    id: OrderId,
    fill_price: Price,
    quantity: Quantity,
}

impl FilledOrder {
    pub fn id(&self) -> OrderId {
        self.id
    }

    pub fn fill_price(&self) -> Price {
        self.fill_price
    }

    pub fn quantity(&self) -> Quantity {
        self.quantity
    }
}

/// Отменённая заявка.
/// Терминальное состояние: методов «снова стать активной» нет.
pub struct CancelledOrder {
    id: OrderId,
}

impl CancelledOrder {
    pub fn id(&self) -> OrderId {
        self.id
    }
}

/// Маркеры состояний phantom-варианта.
/// Собраны в модуль, чтобы не засорять верхнюю область видимости голыми `Working`/`Filled` (см. «Хорошие практики» в статье).
pub mod order_state {
    /// Заявка в стакане — ждёт исполнения.
    pub struct Working;
    /// Заявка исполнена — терминальное состояние.
    pub struct Filled;
}

/// Phantom-вариант той же машины состояний:
/// одна структура на все состояния после постановки, состояние — в параметре типа.
/// Удобен, когда состояния делят почти все поля;
/// черновик остаётся отдельной структурой ([`DraftOrder`]) — у него ещё нет ни id, ни инструмента.
///
/// Compile-fail: метод не своего состояния не вызвать.
///
/// ```compile_fail
/// # use tdd_01_foundations::typestate::{order_state, Order};
/// # use tdd_01_foundations::adt::OrderType;
/// # use tdd_01_foundations::newtype::ids::OrderId;
/// # use tdd_01_foundations::newtype::market::{InstrumentId, InstrumentSpec, TickSize, LotSize, Side};
/// # use rust_decimal::dec;
/// # let spec = InstrumentSpec {
/// #     tick_size: TickSize::new(dec!(0.01)).unwrap(),
/// #     lot_size: LotSize::new(dec!(1)).unwrap(),
/// # };
/// let working = Order::<order_state::Working>::new(
///     OrderId(1),
///     InstrumentId(1),
///     Side::Buy,
///     spec.quantity(dec!(10)).unwrap(),
///     OrderType::Market,
/// );
/// working.settle(); // no method `settle` on Order<Working> — сначала fill
/// ```
pub struct Order<State> {
    id: OrderId,
    instrument: InstrumentId,
    side: Side,
    quantity: Quantity,
    order_type: OrderType,
    _state: PhantomData<State>,
}

/// Generic-методы работают на всех состояниях сразу — одна из причин выбрать
/// phantom-вариант.
impl<State> Order<State> {
    pub fn id(&self) -> OrderId {
        self.id
    }

    pub fn instrument(&self) -> InstrumentId {
        self.instrument
    }

    pub fn side(&self) -> Side {
        self.side
    }

    pub fn quantity(&self) -> Quantity {
        self.quantity
    }

    pub fn order_type(&self) -> OrderType {
        self.order_type
    }
}

impl Order<order_state::Working> {
    /// В реальном коде `Order<Working>` появлялся бы из постановки черновика;
    /// здесь упрощённо — прямой конструктор.
    pub fn new(
        id: OrderId,
        instrument: InstrumentId,
        side: Side,
        quantity: Quantity,
        order_type: OrderType,
    ) -> Self {
        Self {
            id,
            instrument,
            side,
            quantity,
            order_type,
            _state: PhantomData,
        }
    }

    /// Исполнение: меняется только параметр типа — поля переезжают как есть.
    pub fn fill(self) -> Order<order_state::Filled> {
        Order {
            id: self.id,
            instrument: self.instrument,
            side: self.side,
            quantity: self.quantity,
            order_type: self.order_type,
            _state: PhantomData,
        }
    }
}

impl Order<order_state::Filled> {
    /// Расчёты по исполненной заявке.
    /// Здесь упрощённо — ничего не делает.
    pub fn settle(self) {}
}

/// Биржа присваивает id заявке при постановке.
/// Здесь упрощённо — фиксированный id.
fn assign_id() -> OrderId {
    OrderId(1)
}

/// Биржа резолвит символ инструмента (`"AAPL"`) в числовой [`InstrumentId`] на входе.
/// Здесь упрощённо — фиксированный id.
fn resolve(_symbol: &str) -> InstrumentId {
    InstrumentId(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::newtype::market::{InstrumentSpec, LotSize, TickSize};
    use rust_decimal::dec;

    fn spec() -> InstrumentSpec {
        InstrumentSpec {
            tick_size: TickSize::new(dec!(0.01)).unwrap(),
            lot_size: LotSize::new(dec!(1)).unwrap(),
        }
    }

    fn draft() -> DraftOrder {
        let s = spec();
        DraftOrder::limit(
            "AAPL".to_string(),
            Side::Buy,
            s.price(dec!(185.50)).unwrap(),
            s.quantity(dec!(10)).unwrap(),
        )
    }

    #[test]
    fn draft_submits_to_working() {
        let working = draft().submit().expect("accepted");
        assert_eq!(working.id(), OrderId(1));
        assert_eq!(working.instrument(), InstrumentId(1));
        assert!(working.order_type().limit_price().is_some());
    }

    #[test]
    fn working_order_fills_with_exchange_price() {
        let working = draft().submit().unwrap();
        // Цена исполнения приходит от биржи и может отличаться от лимитной (185.50).
        let fill_price = spec().price(dec!(185.49)).unwrap();
        let filled = working.fill(fill_price);
        assert_eq!(filled.id(), OrderId(1));
        assert_eq!(filled.fill_price().amount(), dec!(185.49));
        assert_eq!(filled.quantity().amount(), dec!(10));
    }

    #[test]
    fn working_order_cancels() {
        let cancelled = draft().submit().unwrap().cancel();
        assert_eq!(cancelled.id(), OrderId(1));
    }

    #[test]
    fn typed_constructors_set_order_type() {
        let s = spec();
        let market = DraftOrder::market("AAPL".to_string(), Side::Buy, s.quantity(dec!(10)).unwrap());
        assert!(market.order_type.is_market());

        let stop_limit = DraftOrder::stop_limit(
            "AAPL".to_string(),
            Side::Sell,
            s.price(dec!(180.00)).unwrap(),
            s.price(dec!(181.00)).unwrap(),
            s.quantity(dec!(5)).unwrap(),
        );
        assert_eq!(stop_limit.order_type.limit_price().unwrap().amount(), dec!(181.00));
    }

    #[test]
    fn phantom_variant_carries_fields_across_states() {
        let s = spec();
        let working = Order::<order_state::Working>::new(
            OrderId(7),
            InstrumentId(1),
            Side::Buy,
            s.quantity(dec!(10)).unwrap(),
            OrderType::Market,
        );
        assert!(working.order_type().is_market());

        let filled = working.fill();
        assert_eq!(filled.id(), OrderId(7));
        assert_eq!(filled.quantity().amount(), dec!(10));
        filled.settle();
    }
}
