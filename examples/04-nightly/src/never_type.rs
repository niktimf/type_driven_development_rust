//! Never type `!` в произвольной позиции типа. Стабилизирован 24.08.2026
//! (PR rust-lang/rust#155499), выходит в stable 1.100; `Infallible` стал
//! псевдонимом `!`. `#![feature(never_type)]` на этом nightly даёт
//! предупреждение `stable_features`, поэтому фича не включена.

use std::convert::Infallible;
use std::str::FromStr;

/// `ClientOrderId` части 1, но `Err` записан как `!` напрямую,
/// без `Infallible`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientOrderId(pub String);

impl FromStr for ClientOrderId {
    type Err = !;

    fn from_str(s: &str) -> Result<Self, !> {
        Ok(Self(s.to_string()))
    }
}

/// Разворачивание короче, чем в части 1: там пустая ветка требовала
/// `match never {}`, здесь значение типа `!` приводится к любому типу само.
pub fn parse_client_id(s: &str) -> ClientOrderId {
    match s.parse::<ClientOrderId>() {
        Ok(id) => id,
        Err(never) => never,
    }
}

/// `Infallible` — псевдоним `!`, поэтому `Result<T, Infallible>` части 1
/// и `Result<T, !>` — один тип, и код частей 1 и 2 работает без правок.
pub fn same_type(r: Result<u64, Infallible>) -> Result<u64, !> {
    r
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::size_of;
    use tdd_02_contracts::exchange::{ExchangeClient, SimOrderId, Simulator};
    use tdd_02_contracts::order::tests_support::draft;

    #[test]
    fn never_arm_is_coerced_instead_of_matched_empty() {
        assert_eq!(parse_client_id("order-2026-0001").0, "order-2026-0001");
    }

    #[test]
    fn simulator_result_is_destructured_with_irrefutable_let() {
        let sim = Simulator::default();
        // `type Error = Infallible` у симулятора части 2 — это `!`,
        // и `Err` разбирать не нужно: ни `match`, ни `unwrap`.
        let Ok(SimOrderId(n)) = sim.submit_order(&draft());
        assert_eq!(n, 1);
    }

    #[test]
    fn impossible_variant_takes_no_space() {
        assert_eq!(size_of::<Option<!>>(), 0);
        assert_eq!(size_of::<Result<u64, !>>(), size_of::<u64>());
    }

    #[test]
    fn infallible_is_an_alias() {
        assert_eq!(same_type(Ok(7)), Ok(7));
    }
}
