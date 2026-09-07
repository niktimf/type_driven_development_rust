//! Словарь домена на стороне биржи.
//!
//! Заявка, цены, объёмы и валюта — из частей 1 и 2. Здесь добавлено то, чего
//! у клиента нет: состояние биржи, которое проверки читают, и причины отказа.

use rust_decimal::Decimal;

pub use tdd_01_foundations::newtype::ids::AccountId;
pub use tdd_01_foundations::newtype::market::{Overfill, Remainder};
pub use tdd_01_foundations::phantom::OrderId;
pub use tdd_02_contracts::domain::{
    CancelReason, ClientOrderId, InstrumentId, InstrumentSpec, Money, Price, Quantity, Side, Usd,
};
pub use tdd_02_contracts::order::OrderType;

/// Заявка, собранная слоем приёма. Цена и объём уже проверены спецификацией
/// инструмента (smart constructor из части 1), валюта котировки у биржи одна.
///
/// Без derive-ов: у `ClientOrderId` в части 1 их нет.
pub struct IncomingOrder {
    pub client_id: ClientOrderId,
    pub account: AccountId,
    pub instrument: InstrumentId,
    pub side: Side,
    pub order_type: OrderType<Usd>,
    pub quantity: Quantity,
}

/// Отказ шлюза. Каждой проверке — свой вариант.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rejection {
    Halted,
    OutOfBand,
    NotionalTooLarge,
    PositionLimit,
}

/// Торгуется ли инструмент сейчас.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InstrumentStatus {
    pub halted: bool,
}

/// Состояние рынка по инструменту: последняя сделка и ширина коридора вокруг неё.
#[derive(Debug, Clone, Copy)]
pub struct MarketState {
    pub reference: Price<Usd>,
    pub band: Decimal,
}

/// Лимит по номиналу одной заявки.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub max_notional: Money<Usd>,
}

/// Счёт участника: текущая позиция по инструменту (знак — сторона) и её предел.
#[derive(Debug, Clone, Copy)]
pub struct Account {
    pub position: Decimal,
    pub limit: Decimal,
}

/// Мастер-контекст. Собирается под заявку: статус и рынок её инструмента,
/// лимиты и счёт её участника. Проверки получают из него только свой кусок —
/// см. [`crate::checks::Provide`].
#[derive(Debug, Clone, Copy)]
pub struct ExchangeCtx {
    pub status: InstrumentStatus,
    pub market: MarketState,
    pub limits: Limits,
    pub account: Account,
}

/// Цена заявки для проверок: у рыночной своей нет, берётся последняя сделка.
pub fn effective_price(order_type: OrderType<Usd>, market: &MarketState) -> Price<Usd> {
    match order_type {
        OrderType::Market => market.reference,
        OrderType::Limit(price) | OrderType::StopLimit { limit: price, .. } => price,
    }
}
