//! Раздел «Типы-списки (HList)»: проверки шлюза и прогон списка.
//!
//! Проверка — свой тип с собственным контекстом (GAT из части 2).
//! Мастер-контекст [`ExchangeCtx`] раздаёт проверкам их куски через [`Provide`],
//! а [`RunChecks`] прогоняет список рекурсией по `HNil`/`HCons`.

use crate::domain::{
    Account, ExchangeCtx, IncomingOrder, InstrumentStatus, Limits, MarketState, OrderType,
    Rejection, Side, effective_price,
};
use crate::hlist::{HCons, HNil};

/// Проверка заявки. `Ctx<'a>` — ровно тот кусок состояния биржи, который ей нужен.
pub trait Check {
    type Ctx<'a>;

    fn check(order: &IncomingOrder, ctx: Self::Ctx<'_>) -> Result<(), Rejection>;
}

/// Торгуется ли инструмент. Контекст — только статус.
pub struct HaltCheck;

impl Check for HaltCheck {
    type Ctx<'a> = &'a InstrumentStatus;

    fn check(_order: &IncomingOrder, status: &InstrumentStatus) -> Result<(), Rejection> {
        if status.halted {
            Err(Rejection::Halted)
        } else {
            Ok(())
        }
    }
}

/// Цена в коридоре вокруг последней сделки.
/// У рыночной заявки своей цены нет
pub struct PriceBandCheck;

impl Check for PriceBandCheck {
    type Ctx<'a> = &'a MarketState;

    fn check(order: &IncomingOrder, market: &MarketState) -> Result<(), Rejection> {
        let price = match order.order_type {
            OrderType::Market => return Ok(()),
            OrderType::Limit(price) | OrderType::StopLimit { limit: price, .. } => price,
        };
        let deviation = (price.amount() - market.reference.amount()).abs();
        if deviation > market.band {
            Err(Rejection::OutOfBand)
        } else {
            Ok(())
        }
    }
}

/// Номинал не выше лимита.
/// Контекст — кортеж:
/// номинал рыночной заявки считается по цене последней сделки, одних лимитов проверке мало.
pub struct NotionalLimitCheck;

impl Check for NotionalLimitCheck {
    type Ctx<'a> = (&'a Limits, &'a MarketState);

    fn check(
        order: &IncomingOrder,
        (limits, market): (&Limits, &MarketState),
    ) -> Result<(), Rejection> {
        let notional = effective_price(order.order_type, market) * order.quantity;
        if notional.amount() > limits.max_notional.amount() {
            Err(Rejection::NotionalTooLarge)
        } else {
            Ok(())
        }
    }
}

/// Позиция участника после исполнения не выйдет за предел. Контекст — счёт.
pub struct PositionLimitCheck;

impl Check for PositionLimitCheck {
    type Ctx<'a> = &'a Account;

    fn check(order: &IncomingOrder, account: &Account) -> Result<(), Rejection> {
        let delta = match order.side {
            Side::Buy => order.quantity.amount(),
            Side::Sell => -order.quantity.amount(),
        };
        if (account.position + delta).abs() > account.limit {
            Err(Rejection::PositionLimit)
        } else {
            Ok(())
        }
    }
}

/// Мастер-контекст выдаёт проверке `C` её кусок.
/// По одному `impl` на проверку.
pub trait Provide<C: Check> {
    fn provide(&self) -> C::Ctx<'_>;
}

impl Provide<HaltCheck> for ExchangeCtx {
    fn provide(&self) -> &InstrumentStatus {
        &self.status
    }
}

impl Provide<PriceBandCheck> for ExchangeCtx {
    fn provide(&self) -> &MarketState {
        &self.market
    }
}

impl Provide<NotionalLimitCheck> for ExchangeCtx {
    fn provide(&self) -> (&Limits, &MarketState) {
        (&self.limits, &self.market)
    }
}

impl Provide<PositionLimitCheck> for ExchangeCtx {
    fn provide(&self) -> &Account {
        &self.account
    }
}

/// Прогон списка проверок: база на `HNil`, шаг на `HCons`.
/// Останавливается на первом отказе (`?` в шаге).
///
/// Шаг требует `Ctx: Provide<C>` для каждой проверки в списке.
/// Проверка без `impl Provide` для мастер-контекста не соберётся:
///
/// ```compile_fail
/// use tdd_03_validator::checks::{Check, HaltCheck, PriceBandCheck, RunChecks};
/// use tdd_03_validator::domain::{Account, ExchangeCtx, IncomingOrder, Rejection};
/// use tdd_03_validator::hlist::{HCons, HNil};
///
/// struct SelfTradeCheck;
/// impl Check for SelfTradeCheck {
///     type Ctx<'a> = &'a Account;
///     fn check(_: &IncomingOrder, _: &Account) -> Result<(), Rejection> { Ok(()) }
/// }
/// // `impl Provide<SelfTradeCheck> for ExchangeCtx` забыли.
///
/// type Checks = HCons<HaltCheck, HCons<PriceBandCheck, HCons<SelfTradeCheck, HNil>>>;
///
/// fn run(order: &IncomingOrder, ctx: &ExchangeCtx) -> Result<(), Rejection> {
///     // the trait bound `ExchangeCtx: Provide<SelfTradeCheck>` is not satisfied
///     Checks::run(order, ctx)
/// }
/// ```
pub trait RunChecks<Ctx> {
    fn run(order: &IncomingOrder, ctx: &Ctx) -> Result<(), Rejection>;
}

impl<Ctx> RunChecks<Ctx> for HNil {
    fn run(_order: &IncomingOrder, _ctx: &Ctx) -> Result<(), Rejection> {
        Ok(())
    }
}

impl<C, Tail, Ctx> RunChecks<Ctx> for HCons<C, Tail>
where
    C: Check,
    Ctx: Provide<C>,
    Tail: RunChecks<Ctx>,
{
    fn run(order: &IncomingOrder, ctx: &Ctx) -> Result<(), Rejection> {
        C::check(order, ctx.provide())?;
        Tail::run(order, ctx)
    }
}

/// Боевой контур: все четыре проверки, в порядке от дешёвой к дорогой.
pub type Checks = HCons<
    HaltCheck,
    HCons<PriceBandCheck, HCons<NotionalLimitCheck, HCons<PositionLimitCheck, HNil>>>,
>;

/// Песочница: участники торгуют без реальных денег, проверка позиции выключена.
/// `RunChecks` общий на любой список.
pub type SandboxChecks = HCons<HaltCheck, HCons<PriceBandCheck, HCons<NotionalLimitCheck, HNil>>>;

#[cfg(test)]
pub(crate) mod fixtures {
    use rust_decimal::Decimal;
    use rust_decimal::dec;

    use crate::domain::*;

    pub fn spec() -> InstrumentSpec<Usd> {
        InstrumentSpec::new(
            tdd_02_contracts::domain::TickSize::new(dec!(0.01)).unwrap(),
            tdd_02_contracts::domain::LotSize::new(Decimal::ONE).unwrap(),
        )
    }

    pub fn ctx() -> ExchangeCtx {
        ExchangeCtx {
            status: InstrumentStatus { halted: false },
            market: MarketState {
                reference: spec().price(dec!(100.00)).unwrap(),
                band: dec!(10),
            },
            limits: Limits {
                max_notional: Money::new(dec!(10_000)),
            },
            account: Account {
                position: Decimal::ZERO,
                limit: dec!(100),
            },
        }
    }

    pub fn limit_order(price: Decimal, quantity: Decimal) -> IncomingOrder {
        IncomingOrder {
            client_id: ClientOrderId("cl-1".into()),
            account: AccountId(7),
            instrument: InstrumentId::new(1),
            side: Side::Buy,
            order_type: OrderType::Limit(spec().price(price).unwrap()),
            quantity: spec().quantity(quantity).unwrap(),
        }
    }
}

#[cfg(test)]
mod tests {
    use rust_decimal::dec;

    use super::fixtures::{ctx, limit_order};
    use super::*;

    #[test]
    fn full_chain_passes_a_good_order() {
        assert_eq!(
            Checks::run(&limit_order(dec!(101.00), dec!(5)), &ctx()),
            Ok(())
        );
    }

    #[test]
    fn chain_stops_at_the_first_rejection() {
        let mut ctx = ctx();
        ctx.status.halted = true;
        // Цена вне коридора тоже, но первой стоит проверка статуса.
        let order = limit_order(dec!(150.00), dec!(5));
        assert_eq!(Checks::run(&order, &ctx), Err(Rejection::Halted));
        ctx.status.halted = false;
        assert_eq!(Checks::run(&order, &ctx), Err(Rejection::OutOfBand));
    }

    #[test]
    fn sandbox_skips_the_position_check() {
        let mut ctx = ctx();
        ctx.account.limit = dec!(1);
        let order = limit_order(dec!(101.00), dec!(5));
        assert_eq!(Checks::run(&order, &ctx), Err(Rejection::PositionLimit));
        assert_eq!(SandboxChecks::run(&order, &ctx), Ok(()));
    }
}
