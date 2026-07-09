//! Раздел статьи «ADT (алгебраические типы данных)» — на биржевом домене.
//!
//! - [`Side`] (из [`crate::newtype::market`]) — enum без данных, просто метка.
//! - [`OrderType`] — центральный пример: у `Market` нет поля цены, поэтому
//!   «рыночная заявка с лимитной ценой» в типе невыразима. Показывает все три формы
//!   вариантов сразу (без данных / tuple / именованные поля), методы и `Display`.
//! - [`Order`] — заявка на стороне биржи: тип и цена — внутри `order_type`.
//! - [`OrderEvent`] / [`CancelReason`] — вложенный ADT (внутри варианта — другой enum).

use std::fmt;

use crate::newtype::market::{InstrumentId, Price, Quantity, Side};

/// Тип заявки.
/// Наивно его моделируют через `is_market: bool` и пару `Option`-ов,
/// и тогда из восьми сочетаний осмысленны лишь три:
/// представимы рыночная заявка с ценой или лимитная без цены.
/// Здесь таких состояний нет — у `Market` нет поля цены.
///
/// Показывает три формы вариантов в одном enum:
/// - `Market` — без данных;
/// - `Limit(Price)` — tuple-вариант с одним полем;
/// - `StopLimit { stop, limit }` — несколько именованных полей.
///
/// `match` по `OrderType` обязан покрыть все варианты:
///
/// ```compile_fail
/// use tdd_01_foundations::adt::OrderType;
///
/// fn needs_price(ot: &OrderType) -> bool {
///     match ot {
///         OrderType::Market => false,
///         // не покрыли Limit / StopLimit — error[E0004]: non-exhaustive patterns
///     }
/// }
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderType {
    Market,
    Limit(Price),
    StopLimit { stop: Price, limit: Price },
}

impl OrderType {
    /// Рыночная ли заявка.
    pub fn is_market(&self) -> bool {
        matches!(self, OrderType::Market)
    }

    /// Лимитная цена, если она у этого типа есть. Варианты перечислены явно:
    /// при добавлении нового типа компилятор заставит решить, есть ли у него цена.
    pub fn limit_price(&self) -> Option<Price> {
        match self {
            OrderType::Limit(price) => Some(*price),
            OrderType::StopLimit { limit, .. } => Some(*limit),
            OrderType::Market => None,
        }
    }
}

impl fmt::Display for OrderType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OrderType::Market => write!(f, "market"),
            OrderType::Limit(_) => write!(f, "limit"),
            OrderType::StopLimit { .. } => write!(f, "stop-limit"),
        }
    }
}

/// Заявка на стороне биржи.
/// Тип и цена переехали внутрь [`OrderType`]:
/// `is_market` и `Option`-ы не нужны — их работу делает одно поле `order_type`.
pub struct Order {
    pub instrument: InstrumentId,
    pub side: Side,
    pub quantity: Quantity,
    pub order_type: OrderType,
    // ... другие поля
}

/// Причина отмены — маленький enum, чтобы показать вложение в [`OrderEvent`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancelReason {
    ByUser,
    Expired,
}

/// Вложенный ADT: `OrderEvent::Accepted` содержит внутри другой enum (`OrderType`).
/// `match` остаётся исчерпывающим на любой глубине.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderEvent {
    Accepted {
        order_type: OrderType,
        side: Side,
    },
    Filled {
        price: Price,
        quantity: Quantity,
    },
    Cancelled {
        reason: CancelReason,
    },
}

/// Краткое описание типа заявки — демонстрация `match` по `OrderType`.
pub fn describe_order_type(order_type: &OrderType) -> &'static str {
    match order_type {
        OrderType::Market => "market",
        OrderType::Limit(_) => "limit",
        OrderType::StopLimit { .. } => "stop-limit",
    }
}

/// Двухуровневый `match` из статьи: события пишем через `tracing` — стандарт
/// де-факто для структурированных логов. Появится вариант в `OrderType` — компилятор
/// укажет и на ветки внутри `OrderEvent::Accepted`.
pub fn log_event(event: &OrderEvent) {
    match event {
        OrderEvent::Accepted { order_type, side } => match order_type {
            OrderType::Market =>
                tracing::info!(?side, "accepted market order"),
            OrderType::Limit(price) =>
                tracing::info!(?side, ?price, "accepted limit order"),
            OrderType::StopLimit { stop, limit } =>
                tracing::info!(?side, ?stop, ?limit, "accepted stop-limit"),
        },
        OrderEvent::Filled { price, quantity } =>
            tracing::info!(?price, ?quantity, "filled"),
        OrderEvent::Cancelled { reason } =>
            tracing::info!(?reason, "cancelled"),
    }
}

/// Тестируемый аналог [`log_event`]: та же двухуровневая развилка, но со строкой на выходе.
pub fn describe_event(event: &OrderEvent) -> &'static str {
    match event {
        OrderEvent::Accepted { order_type, .. } => match order_type {
            OrderType::Market => "accepted: market",
            OrderType::Limit(_) => "accepted: limit",
            OrderType::StopLimit { .. } => "accepted: stop-limit",
        },
        OrderEvent::Filled { .. } => "filled",
        OrderEvent::Cancelled { .. } => "cancelled",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::dec;

    fn price(v: &str) -> Price {
        // В тестах собираем Price напрямую через спецификацию инструмента.
        use crate::newtype::market::{InstrumentSpec, LotSize, TickSize};
        let spec = InstrumentSpec {
            tick_size: TickSize::new(dec!(0.01)).unwrap(),
            lot_size: LotSize::new(dec!(1)).unwrap(),
        };
        spec.price(v.parse().unwrap()).unwrap()
    }

    #[test]
    fn market_order_has_no_price() {
        // Ключевой инвариант: у рыночной заявки цены нет в принципе.
        assert_eq!(OrderType::Market.limit_price(), None);
        assert!(OrderType::Market.is_market());
    }

    #[test]
    fn limit_and_stop_limit_expose_their_price() {
        let p = price("185.50");
        assert_eq!(OrderType::Limit(p).limit_price(), Some(p));

        let sl = OrderType::StopLimit {
            stop: price("180.00"),
            limit: price("181.00"),
        };
        assert_eq!(sl.limit_price(), Some(price("181.00")));
        assert!(!sl.is_market());
    }

    #[test]
    fn order_type_displays_its_kind() {
        assert_eq!(OrderType::Market.to_string(), "market");
        assert_eq!(OrderType::Limit(price("185.50")).to_string(), "limit");
    }

    #[test]
    fn nested_match_dispatches_through_both_layers() {
        let events = [
            OrderEvent::Accepted {
                order_type: OrderType::Limit(price("185.50")),
                side: Side::Buy,
            },
            OrderEvent::Accepted {
                order_type: OrderType::Market,
                side: Side::Sell,
            },
            OrderEvent::Filled {
                price: price("185.50"),
                quantity: {
                    use crate::newtype::market::{InstrumentSpec, LotSize, TickSize};
                    let spec = InstrumentSpec {
                        tick_size: TickSize::new(dec!(0.01)).unwrap(),
                        lot_size: LotSize::new(dec!(1)).unwrap(),
                    };
                    spec.quantity(dec!(10)).unwrap()
                },
            },
            OrderEvent::Cancelled {
                reason: CancelReason::ByUser,
            },
        ];

        let descriptions: Vec<_> = events.iter().map(describe_event).collect();
        assert_eq!(
            descriptions,
            ["accepted: limit", "accepted: market", "filled", "cancelled"]
        );
    }

    #[test]
    fn describe_order_type_covers_each_variant() {
        assert_eq!(describe_order_type(&OrderType::Market), "market");
        assert_eq!(describe_order_type(&OrderType::Limit(price("1.00"))), "limit");
        assert_eq!(
            describe_order_type(&OrderType::StopLimit {
                stop: price("1.00"),
                limit: price("1.01"),
            }),
            "stop-limit"
        );
    }
}
