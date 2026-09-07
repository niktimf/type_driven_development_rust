//! Раздел «Event sourcing»: переход как `impl`, событие вместе с переходом,
//! реплей из журнала.
//!
//! Состояние заявки — в типе, как в typestate части 1. Допустимые переходы
//! перечислены `impl`-ами [`Apply`]: их два, оба для `Working`. У `Filled` и
//! `Cancelled` реализаций нет — переходов из них для компилятора не существует.

use std::convert::Infallible;
use std::marker::PhantomData;

use crate::domain::{
    CancelReason, OrderId, OrderType, Overfill, Price, Quantity, Remainder, Side, Usd,
};

/// Запись журнала. `OrderEvent` из части 1 с тремя изменениями: у каждого
/// события есть `order_id` (журнал общий на все заявки), в `Accepted` добавлен
/// `quantity` (по журналу заявку нужно восстановить целиком), цена стала
/// `Price<Usd>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderEvent {
    Accepted {
        order_id: OrderId,
        side: Side,
        order_type: OrderType<Usd>,
        quantity: Quantity,
    },
    Filled {
        order_id: OrderId,
        price: Price<Usd>,
        quantity: Quantity,
    },
    Cancelled {
        order_id: OrderId,
        reason: CancelReason,
    },
}

/// Маркеры состояний. Derive-ы нужны, чтобы derive на `Order<State>` вывелся.
pub mod order_state {
    /// В стакане: ждёт исполнения или отмены.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Working;
    /// Исполнена целиком. Терминальное: `impl Apply` для него нет.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Filled;
    /// Снята. Терминальное.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Cancelled;
}

/// Заявка на стороне биржи. `remaining` — неисполненный остаток: заявка
/// исполняется по частям, и после частичного исполнения остаётся в `Working`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Order<State> {
    id: OrderId,
    side: Side,
    order_type: OrderType<Usd>,
    quantity: Quantity,
    remaining: Quantity,
    _state: PhantomData<State>,
}

impl<State> Order<State> {
    pub fn id(&self) -> OrderId {
        self.id
    }

    pub fn remaining(&self) -> Quantity {
        self.remaining
    }

    /// Смена состояния. Приватная: снаружи переходы идут только через [`Apply`].
    fn transition<Next>(self) -> Order<Next> {
        Order {
            id: self.id,
            side: self.side,
            order_type: self.order_type,
            quantity: self.quantity,
            remaining: self.remaining,
            _state: PhantomData,
        }
    }
}

impl Order<order_state::Working> {
    /// Единственный вход в машину: заявка принята в стакан.
    /// Зовётся из [`crate::gate::accept_into_book`] и из реплея `Accepted`.
    pub(crate) fn accepted(
        id: OrderId,
        side: Side,
        order_type: OrderType<Usd>,
        quantity: Quantity,
    ) -> Self {
        Self {
            id,
            side,
            order_type,
            quantity,
            remaining: quantity,
            _state: PhantomData,
        }
    }
}

/// Исполнение: сколько и почём. Приходит из матчинга, который за кадром.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fill {
    pub price: Price<Usd>,
    pub quantity: Quantity,
}

/// Отмена.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cancel {
    pub reason: CancelReason,
}

/// Переход, параметризованный событием. Меняет тип заявки и одновременно
/// порождает запись для журнала: у `apply` один выход на оба.
/// `Error` — что может пойти не так в самом переходе; у отмены ничего,
/// и там стоит `Infallible` из части 1.
///
/// Для терминальных состояний `impl` нет, и переход из них не компилируется:
///
/// ```compile_fail
/// use tdd_03_validator::domain::CancelReason;
/// use tdd_03_validator::events::{Apply, Cancel, Fill, Order, order_state};
///
/// fn after_cancel(working: Order<order_state::Working>, fill: Fill) {
///     let Ok((cancelled, _event)) = working.apply(Cancel { reason: CancelReason::ByUser });
///     // error[E0599]: no method named `apply` found for struct `Order<State>`
///     //   method not found in `Order<Cancelled>`
///     cancelled.apply(fill);
/// }
/// ```
pub trait Apply<E> {
    type Next;
    type Error;

    fn apply(self, event: E) -> Result<(Self::Next, OrderEvent), Self::Error>;
}

impl Apply<Cancel> for Order<order_state::Working> {
    type Next = Order<order_state::Cancelled>;
    type Error = Infallible;

    fn apply(
        self,
        cancel: Cancel,
    ) -> Result<(Order<order_state::Cancelled>, OrderEvent), Infallible> {
        let event = OrderEvent::Cancelled {
            order_id: self.id,
            reason: cancel.reason,
        };
        Ok((self.transition(), event))
    }
}

/// Исход исполнения. Целевое состояние зависит от остатка объёма, то есть от
/// значения, а `type Next` один на `impl`: тип фиксирует множество исходов,
/// какой из них случится, известно только в рантайме.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FillOutcome {
    Partial(Order<order_state::Working>),
    Full(Order<order_state::Filled>),
}

impl Apply<Fill> for Order<order_state::Working> {
    type Next = FillOutcome;
    /// Объём сверх остатка матчинг не выдаёт, но проверяет это `apply`,
    /// а не договорённость: такой `Fill` — ошибка перехода.
    type Error = Overfill;

    fn apply(self, fill: Fill) -> Result<(FillOutcome, OrderEvent), Overfill> {
        let event = OrderEvent::Filled {
            order_id: self.id,
            price: fill.price,
            quantity: fill.quantity,
        };
        let next = match self.remaining.remaining_after(fill.quantity)? {
            Remainder::Left(remaining) => FillOutcome::Partial(Order {
                remaining,
                ..self.transition()
            }),
            Remainder::Zero => FillOutcome::Full(self.transition()),
        };
        Ok((next, event))
    }
}

/// Текущее состояние при реплее известно только в рантайме — его держит `enum`,
/// но варианты несут типизированные состояния, а не метки.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderState {
    Working(Order<order_state::Working>),
    Filled(Order<order_state::Filled>),
    Cancelled(Order<order_state::Cancelled>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplayError {
    /// История начинается не с `Accepted`.
    NotAccepted,
    /// Пара «состояние и событие», для которой нет `impl Apply`.
    IllegalTransition,
    /// В журнале исполнено больше, чем оставалось в заявке.
    Overfill,
}

impl TryFrom<&OrderEvent> for OrderState {
    type Error = ReplayError;

    fn try_from(first: &OrderEvent) -> Result<Self, ReplayError> {
        match *first {
            OrderEvent::Accepted {
                order_id,
                side,
                order_type,
                quantity,
            } => Ok(Self::Working(Order::accepted(
                order_id, side, order_type, quantity,
            ))),
            OrderEvent::Filled { .. } | OrderEvent::Cancelled { .. } => {
                Err(ReplayError::NotAccepted)
            }
        }
    }
}

impl OrderState {
    /// Один шаг реплея: по событию из журнала зовём `apply` текущего состояния.
    /// Правила переходов не переписаны — внутри веток те же `apply`, что на стороне записи.
    fn step(self, event: &OrderEvent) -> Result<Self, ReplayError> {
        match (self, event) {
            (
                Self::Working(order),
                OrderEvent::Filled {
                    price, quantity, ..
                },
            ) => {
                let (outcome, _already_journaled) = order
                    .apply(Fill {
                        price: *price,
                        quantity: *quantity,
                    })
                    .map_err(|_| ReplayError::Overfill)?;
                Ok(match outcome {
                    FillOutcome::Partial(working) => Self::Working(working),
                    FillOutcome::Full(filled) => Self::Filled(filled),
                })
            }
            (Self::Working(order), OrderEvent::Cancelled { reason, .. }) => {
                // `let Ok(..)` без `match`: `Error` у отмены — `Infallible`,
                // ветки `Err` не существует, как у пустого `match` в части 1.
                let Ok((cancelled, _already_journaled)) = order.apply(Cancel { reason: *reason });
                Ok(Self::Cancelled(cancelled))
            }
            // В позиции состояния `_` нет: новый вариант `OrderState` ломает этот
            // `match` (E0004, как в части 1). Новый вариант `OrderEvent` ловится
            // строкой `Working`, а для терминальных состояний любое событие —
            // отказ, и `_` в позиции события здесь намеренный.
            (Self::Working(_), OrderEvent::Accepted { .. })
            | (Self::Filled(_), _)
            | (Self::Cancelled(_), _) => Err(ReplayError::IllegalTransition),
        }
    }
}

/// Восстановление состояния из журнала: первое событие обязано быть `Accepted`,
/// остальные сворачиваются `try_fold` с ранним выходом по ошибке.
pub fn replay(events: &[OrderEvent]) -> Result<OrderState, ReplayError> {
    let (first, rest) = events.split_first().ok_or(ReplayError::NotAccepted)?;
    rest.iter()
        .try_fold(OrderState::try_from(first)?, OrderState::step)
}

#[cfg(test)]
mod tests {
    use rust_decimal::dec;

    use super::*;
    use crate::checks::Checks;
    use crate::checks::fixtures::{ctx, limit_order, spec};
    use crate::gate::{Gate, accept_into_book};

    fn fill(price: rust_decimal::Decimal, quantity: rust_decimal::Decimal) -> Fill {
        Fill {
            price: spec().price(price).unwrap(),
            quantity: spec().quantity(quantity).unwrap(),
        }
    }

    /// Сторона записи: заявка проходит шлюз, исполняется в два куска, каждый
    /// шаг кладёт событие в журнал. Реплей журнала обязан вернуть ту же заявку.
    #[test]
    fn replay_returns_the_same_order_as_the_write_side() {
        let gate: Gate<Checks> = Gate::new();
        let valid = gate
            .accept(limit_order(dec!(101.00), dec!(5)), &ctx())
            .unwrap();
        let mut journal = Vec::new();

        let (working, accepted) = accept_into_book(valid, OrderId::new(1));
        journal.push(accepted);

        let (outcome, filled) = working.apply(fill(dec!(100.50), dec!(2))).unwrap();
        journal.push(filled);
        let FillOutcome::Partial(working) = outcome else {
            panic!("2 из 5 — частичное исполнение");
        };
        assert_eq!(working.remaining().amount(), dec!(3));

        let (outcome, filled) = working.apply(fill(dec!(100.50), dec!(3))).unwrap();
        journal.push(filled);
        let FillOutcome::Full(filled_order) = outcome else {
            panic!("остатка нет — исполнена целиком");
        };

        assert_eq!(replay(&journal), Ok(OrderState::Filled(filled_order)));
    }

    #[test]
    fn cancel_is_journaled_with_the_transition() {
        let (working, accepted) = accepted_order();
        let Ok((cancelled, event)) = working.apply(Cancel {
            reason: CancelReason::ByUser,
        });
        assert_eq!(
            replay(&[accepted, event]),
            Ok(OrderState::Cancelled(cancelled))
        );
    }

    #[test]
    fn overfill_is_a_transition_error_on_both_sides() {
        let (working, accepted) = accepted_order();
        let too_much = fill(dec!(100.50), dec!(7));
        assert_eq!(
            working.apply(too_much).map(|_| ()),
            Err(Overfill { excess: dec!(2) })
        );

        let filled = OrderEvent::Filled {
            order_id: OrderId::new(1),
            price: too_much.price,
            quantity: too_much.quantity,
        };
        assert_eq!(replay(&[accepted, filled]), Err(ReplayError::Overfill));
    }

    #[test]
    fn journal_from_outside_is_checked_at_runtime() {
        let (_, accepted) = accepted_order();
        let filled = OrderEvent::Filled {
            order_id: OrderId::new(1),
            price: spec().price(dec!(100.50)).unwrap(),
            quantity: spec().quantity(dec!(5)).unwrap(),
        };
        let cancelled = OrderEvent::Cancelled {
            order_id: OrderId::new(1),
            reason: CancelReason::Expired,
        };

        // `Filled` после `Cancelled` — пара без `impl Apply`.
        assert_eq!(
            replay(&[accepted, cancelled, filled]),
            Err(ReplayError::IllegalTransition)
        );
        // История не с `Accepted`.
        assert_eq!(replay(&[filled]), Err(ReplayError::NotAccepted));
        assert_eq!(replay(&[]), Err(ReplayError::NotAccepted));
    }

    fn accepted_order() -> (Order<order_state::Working>, OrderEvent) {
        let gate: Gate<Checks> = Gate::new();
        let valid = gate
            .accept(limit_order(dec!(101.00), dec!(5)), &ctx())
            .unwrap();
        accept_into_book(valid, OrderId::new(1))
    }
}
