//! Заявка на стороне клиента. Продолжение typestate-раздела части 1.

use rust_decimal::Decimal;

use crate::domain::{ClientOrderId, Currency, Price, Quantity, RejectReason, Side};
use crate::exchange::ExchangeClient;

/// Тип заявки. Параметр `Quote` пришёл сюда вместе с [`Price`]: у цены
/// появилась валюта, и параметр обязан стоять на каждом типе, где лежит
/// цена, — см. практику «Параметр типа расползается сам» в статье.
#[derive(Debug, PartialEq, Eq)]
pub enum OrderType<Quote> {
    /// У рыночной заявки цены нет в принципе — поля под неё не существует.
    Market,
    Limit(Price<Quote>),
    StopLimit {
        stop: Price<Quote>,
        limit: Price<Quote>,
    },
}

// Вручную, без bound-а на `Quote`: маркер валюты физически не хранится.
// Часть 1 так же поступает с `Id<Tag>`.
impl<Quote> Clone for OrderType<Quote> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<Quote> Copy for OrderType<Quote> {}

/// Черновик: собран у нас, на площадку ещё не ушёл.
///
/// `ClientOrderId` придумываем мы и кладём в исходящее сообщение. Площадка
/// возвращает его в ответе — по нему и понятно, к какой из отправленных
/// заявок этот ответ относится.
///
/// Без `#[derive(Debug, Clone)]`: у `ClientOrderId` в части 1 их нет, и
/// производные impl-ы не выведутся. Черновик всё равно одноразовый — `submit`
/// его забирает по значению, как в части 1.
pub struct DraftOrder<Quote> {
    client_order_id: ClientOrderId,
    symbol: String,
    side: Side,
    quantity: Quantity,
    order_type: OrderType<Quote>,
}

impl<Quote> DraftOrder<Quote> {
    pub fn market(symbol: String, side: Side, quantity: Quantity) -> Self {
        Self::new(symbol, side, quantity, OrderType::Market)
    }

    pub fn limit(symbol: String, side: Side, price: Price<Quote>, quantity: Quantity) -> Self {
        Self::new(symbol, side, quantity, OrderType::Limit(price))
    }

    pub fn stop_limit(
        symbol: String,
        side: Side,
        stop: Price<Quote>,
        limit: Price<Quote>,
        quantity: Quantity,
    ) -> Self {
        Self::new(symbol, side, quantity, OrderType::StopLimit { stop, limit })
    }

    fn new(symbol: String, side: Side, quantity: Quantity, order_type: OrderType<Quote>) -> Self {
        Self {
            client_order_id: ClientOrderId(format!("cl-{symbol}-{}", side as u8)),
            symbol,
            side,
            quantity,
            order_type,
        }
    }

    pub fn client_order_id(&self) -> &ClientOrderId {
        &self.client_order_id
    }

    pub fn symbol(&self) -> &str {
        &self.symbol
    }

    pub fn side(&self) -> Side {
        self.side
    }

    pub fn quantity(&self) -> Quantity {
        self.quantity
    }

    pub fn order_type(&self) -> &OrderType<Quote> {
        &self.order_type
    }
}

impl<Quote: Currency> DraftOrder<Quote> {
    /// Постановка в стакан. Часть 1 ходила «на биржу» абстрактно — теперь на
    /// входе контракт, и площадка любая, лишь бы котировала в той же валюте.
    ///
    /// Тип идентификатора в результате задаёт площадка:
    /// `WorkingOrder<Usd, RestOrderId>` для REST, `WorkingOrder<Usd, FixOrderId>` для FIX.
    pub fn submit<EC>(
        self,
        exchange_client: &EC,
    ) -> Result<WorkingOrder<Quote, EC::ExchangeOrderId>, RejectReason>
    where
        EC: ExchangeClient<Quote = Quote>,
    {
        // ClientOrderId уже лежит в self — он был присвоен при сборке черновика.
        // Идентификатор площадки существует только после её ответа.
        let exchange_id = exchange_client
            .submit_order(&self)
            .map_err(|_| RejectReason::MarketClosed)?;

        Ok(WorkingOrder::accepted(self, exchange_id))
    }
}

/// Рабочая заявка: площадка приняла. Идентификатора два, и оба нужны.
///
/// `client_id` был до отправки — по нему разбирают ответы площадки.
/// `exchange_id` появился только из ответа — и отменять можно лишь им.
/// В части 1 они были схлопнуты в одно поле `id`;
/// это было упрощение, которое здесь разворачивается.
pub struct WorkingOrder<Quote, Id> {
    client_id: ClientOrderId,
    exchange_id: Id,
    side: Side,
    quantity: Quantity,
    order_type: OrderType<Quote>,
}

impl<Quote, Id> WorkingOrder<Quote, Id> {
    fn accepted(draft: DraftOrder<Quote>, exchange_id: Id) -> Self {
        Self {
            client_id: draft.client_order_id,
            exchange_id,
            side: draft.side,
            quantity: draft.quantity,
            order_type: draft.order_type,
        }
    }

    pub fn client_id(&self) -> &ClientOrderId {
        &self.client_id
    }

    pub fn exchange_id(&self) -> &Id {
        &self.exchange_id
    }

    pub fn side(&self) -> Side {
        self.side
    }

    pub fn quantity(&self) -> Quantity {
        self.quantity
    }

    pub fn order_type(&self) -> &OrderType<Quote> {
        &self.order_type
    }
}

/// Хелперы для тестов других модулей крейта.
///
/// Лежат в публичном API намеренно: валидный [`Quantity`] собирается только
/// через [`crate::domain::InstrumentSpec`],
/// а `#[cfg(test)] mod` между модулями не расшарить.
#[doc(hidden)]
pub mod tests_support {
    use super::*;
    use crate::domain::{InstrumentSpec, LotSize, TickSize, Usd};

    pub fn spec() -> InstrumentSpec<Usd> {
        InstrumentSpec::new(
            TickSize::new(Decimal::new(1, 2)).unwrap(),
            LotSize::new(Decimal::ONE).unwrap(),
        )
    }

    pub fn qty() -> Quantity {
        spec().quantity(Decimal::from(10)).unwrap()
    }

    pub fn draft() -> DraftOrder<Usd> {
        DraftOrder::market("AAPL".into(), Side::Buy, qty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_id_exists_before_the_exchange_answers() {
        let draft = tests_support::draft();
        assert!(draft.client_order_id().0.starts_with("cl-AAPL"));
    }

    #[test]
    fn market_order_has_no_price_field() {
        let draft = tests_support::draft();
        assert_eq!(*draft.order_type(), OrderType::Market);
    }

    #[test]
    fn working_order_keeps_both_ids() {
        use crate::exchange::{RestExchange, RestOrderId};

        let draft = tests_support::draft();
        let our_id = draft.client_order_id().0.clone();

        let working = draft.submit(&RestExchange::default()).unwrap();

        // наш id был до отправки и не изменился
        assert_eq!(working.client_id().0, our_id);
        // id площадки появился только из её ответа
        assert_eq!(*working.exchange_id(), RestOrderId(1));
    }

    #[test]
    fn exchange_id_type_follows_the_exchange() {
        use crate::exchange::{FixExchange, FixOrderId};

        let working = tests_support::draft()
            .submit(&FixExchange::default())
            .unwrap();

        // у FIX-площадки идентификатор строковый — и это видно в типе
        let id: &FixOrderId = working.exchange_id();
        assert_eq!(id.0, "1");
    }
}
