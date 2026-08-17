//! CGP: один контракт — несколько реализаций, выбор за контекстом.
//!
//! Раздел про трейты поставил проблему когерентности:
//! `impl ExchangeClient for RestExchange` в программе может быть ровно один.
//! А способов отправить заявку нужно несколько — боевая REST-площадка,
//! FIX-сессия, детерминированный симулятор для бэктеста.
//! Обычный трейт с обычным `impl` тут не работает:
//! три `impl ExchangeClient for RestExchange` в одном крейте — это ошибка
//! компилятора (E0119), а не три способа поведения на выбор.
//!
//! Context-Generic Programming решает это, расщепляя трейт на две половины:
//!
//! - **consumer trait** (`CanSubmitOrder`) — его вызывает прикладной код,
//!   `context.submit_order(..)`, совершенно не зная, кто именно отправит заявку;
//! - **provider trait** (генерируется макросом `#[cgp_component]`) — его
//!   реализуют сколько угодно раз, по одному разу на площадку, потому что
//!   реализации существуют не *для контекста*, а для отдельных маркерных типов
//!   (`SubmitViaRest`, `SubmitViaFix`, `SubmitToSimulator`).
//!   Когерентность не нарушается: `impl OrderSubmitter<Context> for SubmitViaRest` у каждого
//!   провайдера ровно один, конфликтовать нечему.
//!
//! Выбор провайдера — это `delegate_components!` на конкретном контексте (`TradingApp`, `BacktestApp`).
//! Контекст решает, кто отправляет заявку, ровно один раз, декларативно, а не через `enum` или динамический диспетчер.

use std::convert::Infallible;

use cgp::core::error::ErrorTypeProviderComponent;
use cgp::prelude::*;

use crate::domain::Usd;
use crate::exchange::{
    ExchangeClient, FixExchange, FixOrderId, FixReject, HttpExchangeError, RestExchange, RestOrderId, SimOrderId,
    Simulator,
};
use crate::order::DraftOrder;

/// Абстрактный тип идентификатора после отправки.
/// У каждого контекста свой:
/// у REST — `RestOrderId`, у FIX — `FixOrderId`, у симулятора — `SimOrderId`.
/// Внутри `RestOrderId` и `SimOrderId` одинаковый `u64`, но это разные newtype,
/// и подставить один вместо другого не сможем.
#[cgp_type]
pub trait HasOrderIdType {
    type OrderId;
}

/// Consumer trait: то, что видит прикладной код. `HasErrorType` — готовый
/// абстрактный тип ошибки из `cgp::prelude`, устроен как `HasOrderIdType` выше.
#[cgp_component(OrderSubmitter)]
pub trait CanSubmitOrder: HasOrderIdType + HasErrorType {
    fn submit_order(&self, order: &DraftOrder<Usd>) -> Result<Self::OrderId, Self::Error>;
}

// Провайдеры
//
// Три маркерных типа, три независимых impl-а одного и того же provider trait.
// Ни один из них не реализован «для контекста» — только для себя, поэтому
// когерентность их не касается: добавить четвёртый провайдер (скажем, для
// on-chain площадки) можно в любой момент, не трогая уже существующие.

/// Провайдер №1: боевая REST-площадка.
pub struct SubmitViaRest;

#[cgp_provider]
impl<Context> OrderSubmitter<Context> for SubmitViaRest
where
    Context: HasOrderIdType<OrderId = RestOrderId> + HasErrorType<Error = HttpExchangeError>,
{
    fn submit_order(
        _context: &Context,
        order: &DraftOrder<Usd>,
    ) -> Result<RestOrderId, HttpExchangeError> {
        // В реальном коде соединение с площадкой хранится в контексте и достаётся
        // геттером; здесь для краткости — то же самое соединение, что и в
        // разделе про трейты, поднятое на один вызов.
        RestExchange::default().submit_order(order)
    }
}

/// Провайдер №2: FIX-сессия. Тот же контракт, другой протокол — и другой
/// ассоциированный тип идентификатора у контекста, который его выберет.
pub struct SubmitViaFix;

#[cgp_provider]
impl<Context> OrderSubmitter<Context> for SubmitViaFix
where
    Context: HasOrderIdType<OrderId = FixOrderId> + HasErrorType<Error = FixReject>,
{
    fn submit_order(_context: &Context, order: &DraftOrder<Usd>) -> Result<FixOrderId, FixReject> {
        FixExchange::default().submit_order(order)
    }
}

/// Провайдер №3: детерминированный симулятор для бэктеста. `Error = Infallible`
/// — он не может отказать в принципе, и это часть контракта, а не комментарий.
pub struct SubmitToSimulator;

#[cgp_provider]
impl<Context> OrderSubmitter<Context> for SubmitToSimulator
where
    Context: HasOrderIdType<OrderId = SimOrderId> + HasErrorType<Error = Infallible>,
{
    fn submit_order(_context: &Context, order: &DraftOrder<Usd>) -> Result<SimOrderId, Infallible> {
        Simulator::default().submit_order(order)
    }
}

// Контексты
//
// `delegate_components!` — единственное место, где контекст решает, какой
// провайдер за ним стоит. Выбор происходит при компиляции; `match` по бирже
// во время исполнения не нужен.

/// Продовое приложение: заявки уходят на реальную REST-площадку.
#[derive(Debug, Default)]
pub struct TradingApp;

delegate_components! {
    TradingApp {
        OrderIdTypeProviderComponent: UseType<RestOrderId>,
        ErrorTypeProviderComponent: UseType<HttpExchangeError>,
        OrderSubmitterComponent: SubmitViaRest,
    }
}

/// Бэктест: тот же вызов, тот же интерфейс, но заявки никуда не уходят —
/// их принимает детерминированный симулятор, который не умеет отказывать.
#[derive(Debug, Default)]
pub struct BacktestApp;

delegate_components! {
    BacktestApp {
        OrderIdTypeProviderComponent: UseType<SimOrderId>,
        ErrorTypeProviderComponent: UseType<Infallible>,
        OrderSubmitterComponent: SubmitToSimulator,
    }
}

// `TradingApp`/`BacktestApp` — юнит-структуры сейчас, но `::default()` пишем
// осознанно: в реальном коде контекст обрастёт полями (соединение, конфиг),
// и конструктор не захочется переписывать. Clippy этого не знает.
#[cfg(test)]
#[allow(clippy::default_constructed_unit_structs)]
mod tests {
    use super::*;
    use crate::order::tests_support::draft;

    #[test]
    fn context_picks_the_implementation() {
        let prod = TradingApp::default();
        let backtest = BacktestApp::default();

        // один и тот же вызов, разные реализации под капотом
        assert!(prod.submit_order(&draft()).is_ok());
        assert!(backtest.submit_order(&draft()).is_ok());
    }

    /// Доказательство сильнее, чем «оба вернули `Ok`»: у `TradingApp` и
    /// `BacktestApp` разные ассоциированные типы ошибки, потому что за ними
    /// стоят разные провайдеры. Это видно не по значению, а по типу — и
    /// компилятор проверяет это за нас.
    ///
    /// `Infallible` — необитаемый тип, и exhaustive `match` с пустой веткой
    /// `Err` компилируется только тогда, когда `BacktestApp::Error` — это
    /// действительно `Infallible`. Подключи сюда по ошибке `SubmitViaRest`
    /// вместо `SubmitToSimulator` — `Error` стал бы `HttpExchangeError`,
    /// у него есть обитаемые варианты, и эта же строка перестала бы
    /// компилироваться (E0004, non-exhaustive match). Тест не просто проходит
    /// — он не проходит компиляцию, если контекст выбрал не того провайдера.
    #[test]
    fn backtest_context_really_cannot_fail() {
        let backtest = BacktestApp::default();

        let id = match backtest.submit_order(&draft()) {
            Ok(id) => id,
            Err(never) => match never {},
        };
        assert_eq!(id, SimOrderId(1));
    }

    /// Третий провайдер, третий контекст: FIX собран здесь же, ad hoc, чтобы
    /// показать, что провайдеров действительно можно завести сколько нужно —
    /// добавление `SubmitViaFix` не потребовало трогать ни `SubmitViaRest`,
    /// ни `SubmitToSimulator`, ни трейт `CanSubmitOrder`.
    #[test]
    fn a_third_provider_is_just_another_delegation() {
        #[derive(Default)]
        struct FixApp;

        delegate_components! {
            FixApp {
                OrderIdTypeProviderComponent: UseType<FixOrderId>,
                ErrorTypeProviderComponent: UseType<FixReject>,
                OrderSubmitterComponent: SubmitViaFix,
            }
        }

        let fix = FixApp::default();
        assert_eq!(fix.submit_order(&draft()).unwrap(), FixOrderId("1".into()));
    }
}
