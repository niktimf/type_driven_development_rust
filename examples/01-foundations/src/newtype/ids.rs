//! Простой newtype: одно представление в памяти, разные типы для компилятора.

/// `AccountId` и `OrderId` — обе обёртки над `u64`, но компилятор их различает.
///
/// Пример из статьи: попытка передать `AccountId` в функцию, ожидающую `OrderId`,
/// не компилируется.
///
/// ```compile_fail
/// use tdd_01_foundations::newtype::ids::{AccountId, OrderId};
///
/// fn cancel_order(_id: OrderId) {}
///
/// let account = AccountId(42);
/// cancel_order(account); // expected `OrderId`, found `AccountId`
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccountId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OrderId(pub u64);

impl From<u64> for AccountId {
    fn from(id: u64) -> Self {
        Self(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_u64_works() {
        let id: AccountId = 42.into();
        assert_eq!(id, AccountId(42));
    }

    #[test]
    fn account_id_and_order_id_carry_the_same_payload() {
        let account = AccountId(42);
        let order = OrderId(42);
        assert_eq!(account.0, order.0);
    }
}
