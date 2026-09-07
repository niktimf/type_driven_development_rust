//! Раздел «Типы-списки (HList)» и предикат `Contains` из раздела
//! «Compile-time валидатор».
//!
//! Список из типов собирается из двух структур, как cons-список в Lisp:
//! `HNil` — пустой список, `HCons<Head, Tail>` — элемент и хвост.
//! Любая операция над ним — пара `impl`-ов: база на `HNil`, шаг на `HCons`.
//!
//! В `frunk` то же самое называется `HNil`/`HCons`, а `Contains` — `Selector<S, I>`
//! с индексами `Here`/`There` в `frunk::indices`.

use std::marker::PhantomData;

/// Пустой список.
pub struct HNil;

/// Элемент и хвост; хвост — снова список.
pub struct HCons<Head, Tail>(pub Head, pub Tail);

/// Индекс: искомый элемент — голова списка.
pub struct Here;

/// Индекс: искомый элемент — в хвосте, на индексе `Index`.
pub struct There<Index>(PhantomData<Index>);

/// Утверждение «список содержит `C`». Доказательство — `impl`, второй параметр —
/// индекс, на котором элемент нашёлся; его выводит компилятор.
///
/// Для `HNil` реализации нет: поиск, дошедший до конца, проваливается.
/// Что это даёт, видно на [`crate::gate::Gate::new`] и
/// [`crate::gate::reserve_margin`].
pub trait Contains<C, Index> {}

impl<C, Tail> Contains<C, Here> for HCons<C, Tail> {}

impl<C, Head, Tail, Index> Contains<C, There<Index>> for HCons<Head, Tail> where
    Tail: Contains<C, Index>
{
}

#[cfg(test)]
mod tests {
    use super::*;

    struct A;
    struct B;
    struct C;

    type List = HCons<A, HCons<B, HCons<C, HNil>>>;

    fn index_of<L, T, I>(_list: &L) -> I
    where
        L: Contains<T, I>,
        I: Default,
    {
        I::default()
    }

    impl Default for Here {
        fn default() -> Self {
            Here
        }
    }
    impl<I> Default for There<I> {
        fn default() -> Self {
            There(PhantomData)
        }
    }

    #[test]
    fn index_is_inferred_by_the_compiler() {
        let list: List = HCons(A, HCons(B, HCons(C, HNil)));
        // Индекс руками не пишем — аннотация только проверяет, что вывелось.
        let _: Here = index_of::<_, A, _>(&list);
        let _: There<Here> = index_of::<_, B, _>(&list);
        let _: There<There<Here>> = index_of::<_, C, _>(&list);
    }
}
