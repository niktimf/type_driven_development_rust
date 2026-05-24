//! Раздел статьи «Phantom types» — параметры типа без рантайм-представления.
//!
//! Идея: одна generic-структура [`Id<Tag>`], много типов-маркеров. В памяти
//! `Id<User>` и `Id<Order>` — это один и тот же `u64`, но компилятор их различает.

use std::marker::PhantomData;

/// Generic-идентификатор с phantom-тегом. Все `Id<Tag>` в рантайме — это
/// 8 байт `u64`; разница между маркерами существует только на этапе компиляции.
///
/// Пример из статьи: `UserId` нельзя передать в функцию, ожидающую `OrderId`.
///
/// ```compile_fail
/// use tdd_01_foundations::phantom::{Id, OrderId, UserId};
///
/// fn cancel_order(_id: OrderId) {}
///
/// let user: UserId = Id::new(42);
/// cancel_order(user); // expected `Id<Order>`, found `Id<User>`
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Id<Tag> {
    raw: u64,
    _tag: PhantomData<Tag>,
}

impl<Tag> Id<Tag> {
    pub const fn new(raw: u64) -> Self {
        Self {
            raw,
            _tag: PhantomData,
        }
    }

    pub const fn raw(&self) -> u64 {
        self.raw
    }
}

/// Маркер пользователя. Derive-ы навешены, чтобы `#[derive(...)]` на `Id<Tag>`
/// работал — он добавляет bound `Tag: Debug + Clone + ...` на сгенерированный impl.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct User;

/// Маркер заказа — аналогично.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Order;

pub type UserId = Id<User>;
pub type OrderId = Id<Order>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::size_of;

    #[test]
    fn phantom_does_not_add_runtime_overhead() {
        // PhantomData<Tag> в рантайме отсутствует — размер равен u64.
        assert_eq!(size_of::<Id<User>>(), size_of::<u64>());
        assert_eq!(size_of::<Id<Order>>(), size_of::<u64>());
        assert_eq!(size_of::<UserId>(), 8);
    }

    #[test]
    fn user_id_and_order_id_carry_the_same_payload() {
        let user: UserId = Id::new(42);
        let order: OrderId = Id::new(42);
        // payload одинаковый, типы — разные (что и проверяет compile_fail-доктест).
        assert_eq!(user.raw(), order.raw());
    }

    #[test]
    fn generic_method_works_for_all_tags() {
        // Один impl<Tag> Id<Tag> обслуживает оба варианта без дублирования.
        let user = UserId::new(1);
        let order = OrderId::new(2);
        assert_eq!(user.raw(), 1);
        assert_eq!(order.raw(), 2);
    }

    #[test]
    fn ids_with_same_tag_compare_equal() {
        // Одинаковые tag + одинаковый payload — равны.
        let a: UserId = Id::new(7);
        let b: UserId = Id::new(7);
        assert_eq!(a, b);

        let c: UserId = Id::new(8);
        assert_ne!(a, c);
    }
}
