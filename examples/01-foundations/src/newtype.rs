//! Раздел статьи «Newtype».
//!
//! - [`ids`] — простой newtype без инварианта (`AccountId` / `OrderId`).
//! - [`market`] — newtype с инвариантом: Price / Quantity + smart constructor на
//!   спецификации инструмента (биржевой пример из статьи).
//! - [`secret`] — анти-пример: `Deref` на секретной обёртке (`ApiKey`) обходит маскирующий `Debug`.

pub mod ids;
pub mod market;
pub mod secret;
