//! Раздел статьи «Newtype».
//!
//! - [`ids`] — простой newtype без инварианта (UserId / OrderId).
//! - [`password`] — newtype с инвариантом: argon2-хеш + smart constructor.

pub mod ids;
pub mod password;
