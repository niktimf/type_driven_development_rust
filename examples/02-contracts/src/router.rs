//! Роутер: набор площадок известен из конфига, значит нужен `dyn`.
//! А раз у трейта есть ассоциированные типы, в `dyn`-форме их придётся назвать —
//! отсюда явный слой стирания [`Erased`].

use std::collections::HashMap;
use std::fmt::Display;

use crate::domain::Usd;
use crate::exchange::{Exchange, ExchangeClient};
use crate::order::DraftOrder;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ExchangeName(pub String);

/// Идентификатор заявки после стирания: «какая площадка + её строковый id».
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErasedOrderId {
    pub exchange: ExchangeName,
    pub raw: Box<str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouterError {
    UnknownExchange(ExchangeName),
    Rejected { exchange: &'static str, detail: String },
    /// `ErasedOrderId::raw` не разобрался обратно в нативный id площадки —
    /// например, кто-то передал `raw`, который эта площадка не выдавала.
    UnparseableOrderId(String),
}

/// Приводит типы конкретной площадки к общим типам роутера.
///
/// Стирание одностороннее. `Display` умеет только печатать нативный id в
/// строку (`EC::ExchangeOrderId → ErasedOrderId`); обратной операции у `Display`
/// нет и быть не может — из `"42"` не восстановить, был ли это `u64` или байт
/// строки другого формата. Путь назад поэтому не выводится, а требуется явным
/// bound-ом `TryFrom<ErasedOrderId>`: площадка, которая его не реализовала,
/// просто не пройдёт в `Router::add`.
pub struct Erased<EC>(pub EC);

impl<EC> ExchangeClient for Erased<EC>
where
    EC: ExchangeClient<Quote = Usd> + Exchange,
    EC::ExchangeOrderId: Display + TryFrom<ErasedOrderId>,
    <EC::ExchangeOrderId as TryFrom<ErasedOrderId>>::Error: std::fmt::Debug,
    EC::Error: std::fmt::Debug,
{
    type Quote = Usd;
    type ExchangeOrderId = ErasedOrderId;
    type Error = RouterError;

    fn submit_order(&self, order: &DraftOrder<Usd>) -> Result<ErasedOrderId, RouterError> {
        self.0
            .submit_order(order)
            .map(|id| ErasedOrderId {
                exchange: ExchangeName(EC::NAME.to_string()),
                raw: id.to_string().into_boxed_str(),
            })
            // `format!("{e:?}")` схлопывает структуру ошибки площадки (коды,
            // тексты из `HttpExchangeError`/`FixReject`/`RevertReason`) в
            // строку. Для крейта про «типы несут информацию» это осознанный
            // компромисс: `RouterError` — уже стёртый, общий для всех площадок
            // тип, и без ассоциированной ошибки конкретной площадки ему
            // некуда нести эту структуру, кроме как текстом.
            .map_err(|e| RouterError::Rejected {
                exchange: EC::NAME,
                detail: format!("{e:?}"),
            })
    }

    fn cancel_order(&self, id: ErasedOrderId) -> Result<(), RouterError> {
        let native = EC::ExchangeOrderId::try_from(id)
            .map_err(|e| RouterError::UnparseableOrderId(format!("{e:?}")))?;
        self.0.cancel_order(native).map_err(|e| RouterError::Rejected {
            exchange: EC::NAME,
            detail: format!("{e:?}"),
        })
    }
}

type BoxedClient = Box<
    dyn ExchangeClient<Quote = Usd, ExchangeOrderId = ErasedOrderId, Error = RouterError>
        + Send
        + Sync,
>;

/// Без стирания разные площадки в одну `HashMap` не сложить —
/// generic-параметр это «один тип на всю коллекцию»:
///
/// ```compile_fail
/// use std::collections::HashMap;
/// use tdd_02_contracts::exchange::{FixExchange, RestExchange};
/// use tdd_02_contracts::router::ExchangeName;
///
/// let mut map = HashMap::new();
/// map.insert(ExchangeName("rest".into()), RestExchange::default());
///
/// // E0308: expected `RestExchange`, found `FixExchange`
/// map.insert(ExchangeName("fix".into()), FixExchange::default());
/// ```
#[derive(Default)]
pub struct Router {
    exchanges: HashMap<ExchangeName, BoxedClient>,
}

impl Router {
    pub fn add<EC>(&mut self, exchange: EC)
    where
        EC: ExchangeClient<Quote = Usd> + Exchange + Send + Sync + 'static,
        EC::ExchangeOrderId: Display + TryFrom<ErasedOrderId>,
        <EC::ExchangeOrderId as TryFrom<ErasedOrderId>>::Error: std::fmt::Debug,
        EC::Error: std::fmt::Debug,
    {
        self.exchanges
            .insert(ExchangeName(EC::NAME.to_string()), Box::new(Erased(exchange)));
    }

    pub fn route(
        &self,
        id: &ExchangeName,
        order: &DraftOrder<Usd>,
    ) -> Result<ErasedOrderId, RouterError> {
        self.exchanges
            .get(id)
            .ok_or_else(|| RouterError::UnknownExchange(id.clone()))?
            .submit_order(order)
    }

    pub fn cancel(&self, id: ErasedOrderId) -> Result<(), RouterError> {
        let exchange = self
            .exchanges
            .get(&id.exchange)
            .ok_or_else(|| RouterError::UnknownExchange(id.exchange.clone()))?;
        exchange.cancel_order(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Side, Usd};
    use crate::exchange::{FixExchange, RestExchange};
    use crate::order::{tests_support, DraftOrder};

    #[test]
    fn heterogeneous_exchanges_live_in_one_map() {
        let mut router = Router::default();
        router.add(RestExchange::default());
        router.add(FixExchange::default());

        let draft: DraftOrder<Usd> =
            DraftOrder::market("AAPL".into(), Side::Buy, tests_support::qty());

        let id = router.route(&ExchangeName("rest".into()), &draft).unwrap();
        assert_eq!(id.exchange.0, "rest");
        assert_eq!(&*id.raw, "1");
    }

    #[test]
    fn cancel_forwards_to_the_owning_exchange() {
        let mut router = Router::default();
        router.add(RestExchange::default());

        let draft: DraftOrder<Usd> =
            DraftOrder::market("AAPL".into(), Side::Buy, tests_support::qty());
        let id = router.route(&ExchangeName("rest".into()), &draft).unwrap();

        assert!(router.cancel(id).is_ok());
    }

    #[test]
    fn unparseable_raw_id_is_an_error_not_a_panic() {
        let mut router = Router::default();
        router.add(RestExchange::default());

        // REST разбирает `raw` как `u64` — строка ниже не парсится ни во что.
        let garbage = ErasedOrderId {
            exchange: ExchangeName("rest".into()),
            raw: "not-a-number".into(),
        };

        assert!(matches!(
            router.cancel(garbage),
            Err(RouterError::UnparseableOrderId(_))
        ));
    }
}
