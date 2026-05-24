//! Простой newtype: одно представление в памяти, разные типы для компилятора.

/// `UserId` и `OrderId` — обе обёртки над `u64`, но компилятор их различает.
///
/// Пример из статьи: попытка передать `UserId` в функцию, ожидающую `OrderId`,
/// не компилируется.
///
/// ```compile_fail
/// use tdd_01_foundations::newtype::ids::{UserId, OrderId};
///
/// fn cancel_order(_id: OrderId) {}
///
/// let user = UserId(42);
/// cancel_order(user); // expected `OrderId`, found `UserId`
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UserId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OrderId(pub u64);

impl From<u64> for UserId {
    fn from(id: u64) -> Self {
        Self(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_u64_works() {
        let id: UserId = 42.into();
        assert_eq!(id, UserId(42));
    }

    #[test]
    fn user_id_and_order_id_carry_the_same_payload() {
        let user = UserId(42);
        let order = OrderId(42);
        assert_eq!(user.0, order.0);
    }
}
