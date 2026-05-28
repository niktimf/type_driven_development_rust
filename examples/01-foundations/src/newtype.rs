//! Раздел статьи «Newtype».
//!
//! - [`ids`] — простой newtype без инварианта (UserId / OrderId).
//! - [`market`] — newtype с инвариантом: Price / Quantity + smart constructor на
//!   спецификации инструмента (биржевой пример из статьи).
//! - [`password`] — ещё один newtype с инвариантом: argon2-хеш + smart constructor
//!   на самом типе (классический вариант).

pub mod ids;
pub mod market;
pub mod password;
