//! Сравнение над const-параметрами в bound-е — то, что часть 2 обходила
//! const-блоком.
//!
//! Записать `where N <= E::MAX_BATCH` нельзя ни на каком канале: `where`
//! принимает только баунды. На nightly `generic_const_exprs` разрешает
//! generic-параметры внутри const-выражений, и сравнение прячут в
//! баунд-тип: `Assert<{ N <= E::MAX_BATCH }>: IsTrue`.
//!
//! Отличие от const-блока в части 2 — момент, когда приходит ошибка.
//! Const-блок падает при инстанцировании (post-monomorphization,
//! `cargo check` молчит); баунд проверяется на месте вызова, как любой
//! другой, и `cargo check` его видит.

use tdd_02_contracts::exchange::{Exchange, ExchangeClient};
use tdd_02_contracts::order::DraftOrder;

/// Носитель условия: тип есть только для `true` и `false`, а `IsTrue`
/// реализован только для `Assert<true>`.
pub struct Assert<const COND: bool>;

pub trait IsTrue {}
impl IsTrue for Assert<true> {}

/// Пакет с длиной в типе и лимитом площадки в bound-е.
///
/// Пакет в лимите проходит. Фичу включает и вызывающий крейт: без неё
/// компилятор не вычислит `{ N <= EC::MAX_BATCH }` из чужого bound-а
/// и отвергнет даже корректный вызов:
///
/// ```
/// #![feature(generic_const_exprs)]
/// #![allow(incomplete_features)]
/// use tdd_02_contracts::exchange::RestExchange;
/// use tdd_02_contracts::order::tests_support::draft;
/// use tdd_04_nightly::const_bounds::submit_batch;
///
/// // MAX_BATCH у RestExchange — 5, пакет из трёх подходит.
/// let results = submit_batch(&RestExchange::default(), [draft(), draft(), draft()]);
/// assert_eq!(results.len(), 3);
/// ```
///
/// Пакет больше лимита не компилируется — уже на `cargo check`, на месте вызова:
///
/// ```compile_fail
/// #![feature(generic_const_exprs)]
/// #![allow(incomplete_features)]
/// use tdd_02_contracts::exchange::RestExchange;
/// use tdd_02_contracts::order::tests_support::draft;
/// use tdd_04_nightly::const_bounds::submit_batch;
///
/// // MAX_BATCH у RestExchange — 5, а здесь шесть.
/// // error[E0308]: mismatched types — expected `false`, found `true`
/// //   note: required by a bound in `submit_batch`
/// let _ = submit_batch(
///     &RestExchange::default(),
///     [draft(), draft(), draft(), draft(), draft(), draft()],
/// );
/// ```
pub fn submit_batch<EC, const N: usize>(
    exchange_client: &EC,
    orders: [DraftOrder<EC::Quote>; N],
) -> [Result<EC::ExchangeOrderId, EC::Error>; N]
where
    EC: ExchangeClient + Exchange,
    Assert<{ N <= EC::MAX_BATCH }>: IsTrue,
{
    orders.each_ref().map(|order| exchange_client.submit_order(order))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tdd_02_contracts::exchange::{RestExchange, RestOrderId};
    use tdd_02_contracts::order::tests_support::draft;

    #[test]
    fn batch_within_limit_compiles_and_runs() {
        let rest = RestExchange::default();
        let ids: Vec<_> = submit_batch(&rest, [draft(), draft()])
            .into_iter()
            .map(Result::unwrap)
            .collect();
        assert_eq!(ids, vec![RestOrderId(1), RestOrderId(2)]);
    }
}
