//! Раздел «Compile-time валидатор»: шлюз, который не собрать без обязательных
//! проверок, и проверенная заявка как отдельный тип.

use std::marker::PhantomData;

use crate::checks::{HaltCheck, NotionalLimitCheck, PositionLimitCheck, PriceBandCheck, RunChecks};
use crate::domain::{
    ExchangeCtx, IncomingOrder, MarketState, Money, OrderId, Rejection, Usd, effective_price,
};
use crate::events::{Order, OrderEvent, order_state};
use crate::hlist::Contains;

/// Шлюз, параметризованный списком проверок. Поле приватное: собрать шлюз
/// можно только через [`Gate::new`], а тот требует обязательный минимум.
pub struct Gate<Checks> {
    _checks: PhantomData<Checks>,
}

impl<Checks> Gate<Checks> {
    /// Обязательный минимум: статус инструмента и коридор цены. Индексы `I1`, `I2`
    /// выводит компилятор на вызове, в тип `Gate` они не попадают.
    ///
    /// Без обязательной проверки шлюз не собрать — поиск доходит до `HNil`:
    ///
    /// ```compile_fail
    /// use tdd_03_validator::checks::{HaltCheck, NotionalLimitCheck};
    /// use tdd_03_validator::gate::Gate;
    /// use tdd_03_validator::hlist::{HCons, HNil};
    ///
    /// // the trait bound `HNil: Contains<PriceBandCheck, _>` is not satisfied
    /// let gate: Gate<HCons<HaltCheck, HCons<NotionalLimitCheck, HNil>>> = Gate::new();
    /// ```
    ///
    /// Дубль проверки в списке компилятор называет хуже: `Contains<HaltCheck, _>`
    /// доказывается двумя способами, и он просит аннотацию типа:
    ///
    /// ```compile_fail
    /// use tdd_03_validator::checks::{HaltCheck, PriceBandCheck};
    /// use tdd_03_validator::gate::Gate;
    /// use tdd_03_validator::hlist::{HCons, HNil};
    ///
    /// // error[E0283]: type annotations needed
    /// //   = note: multiple `impl`s satisfying `HCons<HaltCheck, ...>: Contains<HaltCheck, _>` found
    /// let gate: Gate<HCons<HaltCheck, HCons<HaltCheck, HCons<PriceBandCheck, HNil>>>> = Gate::new();
    /// ```
    pub fn new<I1, I2>() -> Self
    where
        Checks: Contains<HaltCheck, I1> + Contains<PriceBandCheck, I2>,
    {
        Gate {
            _checks: PhantomData,
        }
    }

    /// Прогон меняет тип: на выходе не `IncomingOrder`, а [`Valid`].
    pub fn accept(
        &self,
        order: IncomingOrder,
        ctx: &ExchangeCtx,
    ) -> Result<Valid<Checks>, Rejection>
    where
        Checks: RunChecks<ExchangeCtx>,
    {
        Checks::run(&order, ctx)?;
        Ok(Valid {
            order,
            _checks: PhantomData,
        })
    }
}

/// Заявка вместе с доказательством, что она прошла цепочку `Checks`.
/// Поля приватные, выдаёт `Valid` только [`Gate::accept`]: smart constructor
/// из части 1, только доказывает он факт прогона, а не свойство значения.
pub struct Valid<Checks> {
    order: IncomingOrder,
    _checks: PhantomData<Checks>,
}

impl<Checks> Valid<Checks> {
    pub fn order(&self) -> &IncomingOrder {
        &self.order
    }
}

/// Постановка в стакан. Требует проверку номинала — и только её.
/// Возвращает заявку в стакане вместе с событием `Accepted` для журнала:
/// событие появляется вместе с переходом, как у [`crate::events::Apply`].
/// Сам матчинг за кадром.
pub fn accept_into_book<Checks, I>(
    valid: Valid<Checks>,
    id: OrderId,
) -> (Order<order_state::Working>, OrderEvent)
where
    Checks: Contains<NotionalLimitCheck, I>,
{
    let order = valid.order;
    let event = OrderEvent::Accepted {
        order_id: id,
        side: order.side,
        order_type: order.order_type,
        quantity: order.quantity,
    };
    (
        Order::accepted(id, order.side, order.order_type, order.quantity),
        event,
    )
}

/// Маржинальный расчёт. Требует проверку позиции — и только её.
///
/// Пропуск от шлюза, в списке которого нет `PositionLimitCheck`, сюда не примут:
///
/// ```compile_fail
/// use tdd_03_validator::checks::SandboxChecks;
/// use tdd_03_validator::domain::{ExchangeCtx, IncomingOrder, MarketState, Money, Usd};
/// use tdd_03_validator::gate::{Gate, Valid, reserve_margin};
///
/// fn margin(valid: &Valid<SandboxChecks>, market: &MarketState) -> Money<Usd> {
///     // the trait bound `HNil: Contains<PositionLimitCheck, _>` is not satisfied
///     reserve_margin(valid, market)
/// }
/// ```
pub fn reserve_margin<Checks, I>(valid: &Valid<Checks>, market: &MarketState) -> Money<Usd>
where
    Checks: Contains<PositionLimitCheck, I>,
{
    // Упрощение: резервируем полный номинал.
    effective_price(valid.order.order_type, market) * valid.order.quantity
}

#[cfg(test)]
mod tests {
    use rust_decimal::dec;

    use super::*;
    use crate::checks::fixtures::{ctx, limit_order};
    use crate::checks::{Checks, SandboxChecks};

    #[test]
    fn live_gate_accepts_and_both_consumers_take_the_pass() {
        let gate: Gate<Checks> = Gate::new();
        let ctx = ctx();
        let valid = gate
            .accept(limit_order(dec!(101.00), dec!(5)), &ctx)
            .unwrap();

        assert_eq!(reserve_margin(&valid, &ctx.market).amount(), dec!(505.00));
        let (working, event) = accept_into_book(valid, OrderId::new(1));
        assert_eq!(working.id(), OrderId::new(1));
        assert!(
            matches!(event, OrderEvent::Accepted { order_id, .. } if order_id == OrderId::new(1))
        );
    }

    #[test]
    fn sandbox_gate_builds_and_feeds_the_book() {
        // Проверки позиции нет, `Gate::new` её и не требует.
        let gate: Gate<SandboxChecks> = Gate::new();
        let valid = gate
            .accept(limit_order(dec!(101.00), dec!(5)), &ctx())
            .unwrap();
        let (working, _) = accept_into_book(valid, OrderId::new(2));
        assert_eq!(working.id(), OrderId::new(2));
    }

    #[test]
    fn rejection_comes_from_the_first_failed_check() {
        let gate: Gate<Checks> = Gate::new();
        let order = limit_order(dec!(101.00), dec!(500));
        assert_eq!(
            gate.accept(order, &ctx()).map(|_| ()),
            Err(Rejection::NotionalTooLarge)
        );
    }
}
