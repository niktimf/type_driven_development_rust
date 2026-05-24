# Источники и ссылки

> Ссылки из второй колонки агент сгенерировал по памяти без онлайн-проверки.
> Перед публикацией статьи прогнать каждую через браузер.

## Книги

- Гранин А., «Проектирование на уровне типов. Системный взгляд на дизайн и архитектуру», ДМК Пресс 2025 — глава «Розеттский камень. Глава 1. Rust» (стр. 188+).

## Часть 1 — Основы (newtype, ADT, uninhabited, phantom, typestate)

- [The Typestate Pattern in Rust](http://cliffle.com/blog/rust-typestate/) — Cliff Biffle. Канонический разбор typestate на примере state-machine для устройства. _(проверить)_
- [Pretty State Machine Patterns in Rust](https://hoverbear.org/blog/rust-state-machine-pattern/) — Hoverbear. Несколько способов закодировать конечный автомат через phantom types. _(проверить)_
- [State Machines](https://blog.yoshuawuyts.com/state-machines/) — Yosh Wuyts. Современный взгляд на ту же тему, zero-cost-гарантии. _(проверить)_
- [`!` (never type)](https://doc.rust-lang.org/std/primitive.never.html) — официальные docs по uninhabited типу.
- [Tracking issue for `!`](https://github.com/rust-lang/rust/issues/35121) — статус стабилизации never type.

## Часть 2 — Контракты (трейты, ассоциированные типы, const generics)

- [Associated Type Constructors, Part 1](https://smallcultfollowing.com/babysteps/blog/2016/11/02/associated-type-constructors-part-1-basic-idea-and-some-recap/) — Niko Matsakis. База ассоциированных типов и обобщение до GATs. _(проверить)_
- [Generic Associated Types to be stable in Rust 1.65](https://blog.rust-lang.org/2022/10/28/gats-stabilization/) — Jack Huey, Rust lang team. _(проверить)_
- [Shipping const generics in 2020](https://without.boats/blog/shipping-const-generics/) — without.boats о дизайне и мотивации `min_const_generics`. _(проверить)_
- [Const generics MVP beta](https://blog.rust-lang.org/2021/02/26/const-generics-mvp-beta.html) — Rust team announcement. _(проверить)_

## Часть 3 — Валидатор (типы-списки, compile-time валидаторы, event sourcing)

- [Type-Level Programming in Rust](https://willcrichton.net/notes/type-level-programming/) — Will Crichton. Peano-числа, HList, рекурсивная диспетчеризация трейтов. _(проверить)_
- [Gentle intro to type-level recursion via frunk HList](https://beachape.com/blog/2017/03/12/gentle-intro-to-type-level-recursion-in-Rust-from-zero-to-frunk-hlist-sculpting/) — Lloyd Chan. _(проверить)_
- [`frunk` crate docs](https://docs.rs/frunk/) — де-факто библиотека HList/Coproduct.
- [`static_assertions` crate](https://docs.rs/static_assertions/) — макросы compile-time-проверок.

## Часть 4 — Nightly (pattern types, const traits, gen-блоки, эффекты)

- [A grand vision for Rust — effects](https://blog.yoshuawuyts.com/a-grand-vision-for-rust/#effects) — Yosh Wuyts. О направлении языка в сторону алгебраических эффектов.
- [Extending Rust's Effect System](https://blog.yoshuawuyts.com/extending-rusts-effect-system/) — Yosh Wuyts. Прямое продолжение «grand vision», про keyword-generics и эффект-полиморфизм. _(проверить)_
- [Coroutines, async and iter](https://without.boats/blog/coroutines-async-and-iter/) — without.boats. Связь корутин, `gen`-блоков и async. _(проверить)_
- [RFC 3513 — gen blocks](https://github.com/rust-lang/rfcs/pull/3513) — официальный спек gen-блоков. _(проверить)_
- [RFC pattern types](https://github.com/rust-lang/rfcs/pull/3535) — pattern types proposal. _(проверить)_
- [Tracking issue: const traits](https://github.com/rust-lang/rust/issues/67792) — `const_trait_impl`. _(проверить)_