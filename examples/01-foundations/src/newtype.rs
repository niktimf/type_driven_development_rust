//! Раздел статьи «Newtype».
//!
//! - [`ids`] — простой newtype без инварианта (AccountId / OrderId).
//! - [`market`] — newtype с инвариантом: Price / Quantity + smart constructor на
//!   спецификации инструмента (биржевой пример из статьи).

pub mod ids;
pub mod market;
