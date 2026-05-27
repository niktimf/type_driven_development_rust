# Источники и ссылки

> Ссылки из второй колонки агент сгенерировал по памяти без онлайн-проверки.
> Перед публикацией статьи прогнать каждую через браузер.

## Книги

- Гранин А., «Проектирование на уровне типов. Системный взгляд на дизайн и архитектуру», ДМК Пресс, 19.08.2025, ISBN 978-5-93700-379-9 — глава «Розеттский камень. Глава 1. Rust» (стр. 188+).

## Часть 1 — Основы (newtype, ADT, uninhabited, phantom, typestate)

- [The Typestate Pattern in Rust](http://cliffle.com/blog/rust-typestate/) — Cliff Biffle, 2019. Канонический разбор typestate на примере HTTP response builder (порядок: статус-строка → заголовки → тело).
- [Pretty State Machine Patterns in Rust](https://hoverbear.org/blog/rust-state-machine-pattern/) — Hoverbear. Несколько способов закодировать конечный автомат: enum-обёртки, отдельные struct-ы, generic-параметры состояния.
- [State Machines: Introduction](https://blog.yoshuawuyts.com/state-machines/) — Yosh Wuyts, 2020. Современный взгляд на ту же тему.
- [`!` (never type)](https://doc.rust-lang.org/std/primitive.never.html) — официальные docs по uninhabited типу.
- [Tracking issue for `!`](https://github.com/rust-lang/rust/issues/35121) — статус стабилизации never type.

## Часть 2 — Контракты (трейты, ассоциированные типы, const generics)

- [Associated Type Constructors, Part 1](https://smallcultfollowing.com/babysteps/blog/2016/11/02/associated-type-constructors-part-1-basic-concepts-and-introduction/) — Niko Matsakis, 2016. База ассоциированных типов и обобщение до GATs.
- [Generic Associated Types to be stable in Rust 1.65](https://blog.rust-lang.org/2022/10/28/gats-stabilization/) — Jack Huey, Rust Types Team, 28.10.2022.
- [Shipping const generics in 2020](https://without.boats/blog/shipping-const-generics/) — without.boats, 16.07.2020. О дизайне и мотивации `min_const_generics`.
- [Const generics MVP beta](https://blog.rust-lang.org/2021/02/26/const-generics-mvp-beta.html) — Rust team announcement, 26.02.2021.
- [CGP — Context-Generic Programming](https://github.com/contextgeneric/cgp) — модульная парадигма поверх traits + associated types, требует Rust 1.81+. Работа над CGP началась в июле 2022 при разработке Hermes IBC Relayer в Informal Systems; сейчас используется в [hermes-sdk](https://github.com/informalsystems/hermes-sdk) (новой версии relayer-а), не в оригинальном [hermes](https://github.com/informalsystems/hermes).
- [contextgeneric.dev](https://contextgeneric.dev) — доки CGP и книга «Context-Generic Programming Patterns».

## Часть 3 — Валидатор (типы-списки, compile-time валидаторы, event sourcing)

- [Type-Level Programming in Rust](https://willcrichton.net/notes/type-level-programming/) — Will Crichton, 24.04.2020. Peano-числа, тип-уровень список через кортежи `(T, L)`, рекурсивная диспетчеризация трейтов.
- [Gentle Intro to Type-level Recursion in Rust: From Zero to HList Sculpting](https://beachape.com/blog/2017/03/12/gentle-intro-to-type-level-recursion-in-Rust-from-zero-to-frunk-hlist-sculpting/) — Lloyd Chan, 12.03.2017.
- [`frunk` crate docs](https://docs.rs/frunk/) — де-факто библиотека HList/Coproduct.
- [`static_assertions` crate](https://docs.rs/static_assertions/) — макросы compile-time-проверок.

## Часть 4 — Nightly (pattern types, const traits, gen-блоки, эффекты)

- [A grand vision for Rust — effects](https://blog.yoshuawuyts.com/a-grand-vision-for-rust/#effects) — Yosh Wuyts. О направлении языка в сторону алгебраических эффектов.
- [Extending Rust's Effect System](https://blog.yoshuawuyts.com/extending-rusts-effect-system/) — Yosh Wuyts. Прямое продолжение «grand vision», про эффект-полиморфизм («effect generics»).
- [Coroutines, async and iter](https://without.boats/blog/coroutines-async-and-iter/) — without.boats. Связь корутин, `gen`-блоков и async.
- [RFC 3513 — gen blocks](https://github.com/rust-lang/rfcs/pull/3513) — официальный спек gen-блоков, merged 07.04.2024, резервирует `gen` в Rust 2024.
- [MCP pattern types (types-team #126)](https://github.com/rust-lang/types-team/issues/126) — Major Change Proposal по pattern types (oli-obk, 18.01.2024). Полноценного RFC PR пока нет; формат — MCP, а не RFC.
- [Tracking issue: const traits](https://github.com/rust-lang/rust/issues/67792) — `const_trait_impl`.