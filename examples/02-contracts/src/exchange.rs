//! Контракт площадки и его реализации.
//!
//! Площадки названы по протоколу, а не по бренду: формат идентификатора
//! определяет протокол, а не рынок. Отсюда и разные ассоциированные типы —
//! число, строка и 32 байта в один тип не сводятся.

use std::convert::Infallible;
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::domain::Currency;
use crate::order::DraftOrder;
use crate::router::ErasedOrderId;

/// `ErasedOrderId` не разбирается обратно сама — площадка обязана предъявить
/// путь назад через `TryFrom`. Одна и та же ошибка на все три нативных типа:
/// само значение всё равно теряется, важен только факт «строка не подошла».
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeIdParseError {
    /// Строка, которую не удалось разобрать.
    pub raw: Box<str>,
}

/// Метаданные площадки. Вынесено отдельным трейтом не случайно:
/// ассоциированная константа делает трейт dyn-несовместимым, а
/// [`ExchangeClient`] нам ещё понадобится в `Box<dyn ...>`.
pub trait Exchange {
    const NAME: &'static str;
    /// Сколько заявок площадка принимает одним пакетом.
    const MAX_BATCH: usize;
}

/// Контракт: «умею принять и отменить заявку».
pub trait ExchangeClient {
    /// Валюта котировки, в которой площадка принимает заявки.
    type Quote: Currency;
    /// Идентификатор в родном формате площадки.
    type ExchangeOrderId;
    type Error;

    fn submit_order(
        &self,
        order: &DraftOrder<Self::Quote>,
    ) -> Result<Self::ExchangeOrderId, Self::Error>;

    /// Отменить можно только тем id, который вернула площадка:
    /// своего `ClientOrderId` для этого недостаточно.
    fn cancel_order(&self, id: Self::ExchangeOrderId) -> Result<(), Self::Error>;

    /// Пакетная постановка. По умолчанию — просто цикл по одиночным.
    /// Площадка с batch-эндпоинтом переопределит; остальных устраивает цикл.
    fn submit_batch(
        &self,
        orders: &[DraftOrder<Self::Quote>],
    ) -> Vec<Result<Self::ExchangeOrderId, Self::Error>> {
        orders.iter().map(|order| self.submit_order(order)).collect()
    }
}

// ─── JSON-over-HTTP ──────────────────────────────────────────────────────────

/// Числовой идентификатор в теле ответа.
#[derive(Debug, Default)]
pub struct RestExchange {
    next_id: AtomicU64,
}

/// Идентификатор REST-площадки. Внутри `u64`, но это не `u64`: у симулятора
/// ниже точно такое же внутреннее представление, и путать их нельзя — та же
/// защита, с которой часть 1 начиналась на паре `AccountId` / `OrderId`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RestOrderId(pub u64);

impl fmt::Display for RestOrderId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HttpExchangeError {
    Status { code: u16 },
    Rejected { code: i32, msg: String },
}

impl Exchange for RestExchange {
    const NAME: &'static str = "rest";
    const MAX_BATCH: usize = 5;
}

impl ExchangeClient for RestExchange {
    type Quote = crate::domain::Usd;
    type ExchangeOrderId = RestOrderId;
    type Error = HttpExchangeError;

    fn submit_order(
        &self,
        _order: &DraftOrder<Self::Quote>,
    ) -> Result<RestOrderId, HttpExchangeError> {
        Ok(RestOrderId(self.next_id.fetch_add(1, Ordering::Relaxed) + 1))
    }

    fn cancel_order(&self, _id: RestOrderId) -> Result<(), HttpExchangeError> {
        Ok(())
    }
}

/// Обратный путь для REST: строка должна быть тем же числом, которое сама
/// площадка когда-то напечатала через `Display`.
impl TryFrom<ErasedOrderId> for RestOrderId {
    type Error = NativeIdParseError;

    fn try_from(id: ErasedOrderId) -> Result<Self, Self::Error> {
        match id.raw.parse() {
            Ok(raw) => Ok(RestOrderId(raw)),
            Err(_) => Err(NativeIdParseError { raw: id.raw }),
        }
    }
}

// ─── FIX ─────────────────────────────────────────────────────────────────────

/// FIX-сессия: идентификатор — строка, тег 37 `OrderID`.
#[derive(Debug, Default)]
pub struct FixExchange {
    next_id: AtomicU64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixOrderId(pub String);

impl fmt::Display for FixOrderId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Тег 103 `OrdRejReason` плюс текст из тега 58.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixReject {
    pub reason: u16,
    pub text: String,
}

impl Exchange for FixExchange {
    const NAME: &'static str = "fix";
    const MAX_BATCH: usize = 1;
}

impl ExchangeClient for FixExchange {
    type Quote = crate::domain::Usd;
    type ExchangeOrderId = FixOrderId;
    type Error = FixReject;

    fn submit_order(&self, _order: &DraftOrder<Self::Quote>) -> Result<FixOrderId, FixReject> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        Ok(FixOrderId(id.to_string()))
    }

    fn cancel_order(&self, _id: FixOrderId) -> Result<(), FixReject> {
        Ok(())
    }
}

/// Обратный путь для FIX: нативный id уже строка, разбор тривиален и не может
/// провалиться — но сигнатура `TryFrom` у роутера одна на все площадки.
impl TryFrom<ErasedOrderId> for FixOrderId {
    type Error = NativeIdParseError;

    fn try_from(id: ErasedOrderId) -> Result<Self, Self::Error> {
        Ok(FixOrderId(String::from(id.raw)))
    }
}

// ─── On-chain ────────────────────────────────────────────────────────────────

/// DEX: идентификатор — хеш транзакции.
#[derive(Debug, Default)]
pub struct OnChainExchange;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TxHash(pub [u8; 32]);

impl fmt::Display for TxHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RevertReason {
    SlippageTooHigh,
    OutOfGas,
}

impl Exchange for OnChainExchange {
    const NAME: &'static str = "onchain";
    const MAX_BATCH: usize = 1;
}

impl ExchangeClient for OnChainExchange {
    type Quote = crate::domain::Usd;
    type ExchangeOrderId = TxHash;
    type Error = RevertReason;

    fn submit_order(&self, _order: &DraftOrder<Self::Quote>) -> Result<TxHash, RevertReason> {
        Ok(TxHash([0u8; 32]))
    }

    fn cancel_order(&self, _id: TxHash) -> Result<(), RevertReason> {
        Ok(())
    }
}

/// Обратный путь для on-chain: 32 байта — это ровно 64 hex-символа, ни больше
/// ни меньше, и каждая пара символов обязана быть валидным байтом.
impl TryFrom<ErasedOrderId> for TxHash {
    type Error = NativeIdParseError;

    fn try_from(id: ErasedOrderId) -> Result<Self, Self::Error> {
        let bytes = id.raw.as_bytes();
        if bytes.len() != 64 {
            return Err(NativeIdParseError { raw: id.raw });
        }
        let mut out = [0u8; 32];
        for (byte, chunk) in out.iter_mut().zip(bytes.chunks_exact(2)) {
            let malformed = || NativeIdParseError { raw: id.raw.clone() };
            let hex = std::str::from_utf8(chunk).map_err(|_| malformed())?;
            *byte = u8::from_str_radix(hex, 16).map_err(|_| malformed())?;
        }
        Ok(TxHash(out))
    }
}

// ─── Симулятор ───────────────────────────────────────────────────────────────

/// Детерминированный симулятор для бэктеста: отказать не может в принципе.
/// `Infallible` из части 1 — ровно про это, и в части 4 он станет `!`.
/// Идентификатор симулятора. Внутри тот же `u64`, что у [`RestOrderId`], —
/// и это разные типы: подставить один вместо другого компилятор не даст.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SimOrderId(pub u64);

impl fmt::Display for SimOrderId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl TryFrom<ErasedOrderId> for SimOrderId {
    type Error = NativeIdParseError;

    fn try_from(id: ErasedOrderId) -> Result<Self, Self::Error> {
        match id.raw.parse() {
            Ok(raw) => Ok(SimOrderId(raw)),
            Err(_) => Err(NativeIdParseError { raw: id.raw }),
        }
    }
}

#[derive(Debug, Default)]
pub struct Simulator {
    next_id: AtomicU64,
}

impl Exchange for Simulator {
    const NAME: &'static str = "sim";
    const MAX_BATCH: usize = usize::MAX;
}

impl ExchangeClient for Simulator {
    type Quote = crate::domain::Usd;
    type ExchangeOrderId = SimOrderId;
    type Error = Infallible;

    fn submit_order(&self, _order: &DraftOrder<Self::Quote>) -> Result<SimOrderId, Infallible> {
        Ok(SimOrderId(self.next_id.fetch_add(1, Ordering::Relaxed) + 1))
    }

    fn cancel_order(&self, _id: SimOrderId) -> Result<(), Infallible> {
        Ok(())
    }
}

/// Пакет с длиной в типе. Связать `N` с [`Exchange::MAX_BATCH`] через bound
/// на стабильном Rust нельзя — `where N <= Self::MAX_BATCH` это
/// `generic_const_exprs`, он до сих пор в nightly. Остаётся const-блок.
///
/// Оговорка существенная: это post-monomorphization error. Она возникает
/// не на сигнатуре, а при инстанцировании, и `cargo check` её не увидит —
/// нужен именно `cargo build` (или запуск теста, который его вызывает).
pub fn submit_batch_typed<EC, const N: usize>(
    exchange_client: &EC,
    orders: [DraftOrder<EC::Quote>; N],
) -> [Result<EC::ExchangeOrderId, EC::Error>; N]
where
    EC: ExchangeClient + Exchange,
{
    const { assert!(N <= EC::MAX_BATCH, "batch exceeds exchange limit") };
    orders.each_ref().map(|order| exchange_client.submit_order(order))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::order::tests_support::draft;

    #[test]
    fn each_exchange_returns_its_own_id_shape() {
        assert_eq!(
            RestExchange::default().submit_order(&draft()).unwrap(),
            RestOrderId(1)
        );
        assert_eq!(
            FixExchange::default().submit_order(&draft()).unwrap(),
            FixOrderId("1".into())
        );
        assert_eq!(
            OnChainExchange.submit_order(&draft()).unwrap(),
            TxHash([0u8; 32])
        );
    }

    #[test]
    fn simulator_cannot_fail() {
        let sim = Simulator::default();
        // type Error = Infallible: у ветки Err нет ни одного обитателя,
        // и пустой match это доказывает. `let Ok(..)` без else здесь не годится.
        let id = match sim.submit_order(&draft()) {
            Ok(id) => id,
            Err(never) => match never {},
        };
        assert_eq!(id, SimOrderId(1));
    }

    #[test]
    fn default_batch_is_a_loop_over_singles() {
        let rest = RestExchange::default();
        let orders = [draft(), draft(), draft()];
        let ids: Vec<_> = rest
            .submit_batch(&orders)
            .into_iter()
            .map(Result::unwrap)
            .collect();
        assert_eq!(ids, vec![RestOrderId(1), RestOrderId(2), RestOrderId(3)]);
    }

    #[test]
    fn typed_batch_keeps_length_in_the_type() {
        let rest = RestExchange::default();
        let batch = [draft(), draft(), draft()];
        let results = submit_batch_typed(&rest, batch);
        // длина результата известна компилятору
        let _: [Result<RestOrderId, HttpExchangeError>; 3] = results;
    }
}
