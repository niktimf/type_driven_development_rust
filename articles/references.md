# Источники и ссылки

> Часть ссылок ещё требует проверки перед публикацией.
> Источники части 4 с отметкой 2026-10-03 проверены при редактуре этой статьи.
> Поведение примеров относится к закреплённому nightly-2026-08-31.

## Книги

- Гранин А., «Проектирование на уровне типов. Системный взгляд на дизайн и архитектуру», ДМК Пресс, 19.08.2025, ISBN 978-5-93700-379-9 — глава «Розеттский камень. Глава 1. Rust» (стр. 188+).

## Часть 1 — Основы (newtype, ADT, uninhabited, phantom, typestate)

- [The Typestate Pattern in Rust](http://cliffle.com/blog/rust-typestate/) — Cliff Biffle, 2019. Канонический разбор typestate на примере HTTP response builder (порядок: статус-строка -> заголовки -> тело).
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

## Часть 3 — Валидатор (type-level lists (HList), compile-time validators, event sourcing)

- [Type-Level Programming in Rust](https://willcrichton.net/notes/type-level-programming/) — Will Crichton, 24.04.2020. Peano-числа, тип-уровень список через кортежи `(T, L)`, рекурсивная диспетчеризация трейтов.
- [Gentle Intro to Type-level Recursion in Rust: From Zero to HList Sculpting](https://beachape.com/blog/2017/03/12/gentle-intro-to-type-level-recursion-in-Rust-from-zero-to-frunk-hlist-sculpting/) — Lloyd Chan, 12.03.2017.
- [`frunk` crate docs](https://docs.rs/frunk/) — HList (`HCons`/`HNil`, `hlist!`), `Generic`, `Coproduct`, `Validated`; версия 0.5.0, стабильный Rust без `unsafe`. Проверено по docs.rs 2026-08-18.
- [`typenum` crate docs](https://docs.rs/typenum/) — числа на уровне типов (`UInt`/`UTerm`, `consts::U0..U1024`), мост в const generics через `generic_const_mappings`; версия 1.20.1. Проверено по docs.rs 2026-08-18.
- [`typed-builder` crate docs](https://docs.rs/typed-builder/) — type-state builder: обязательные поля отслеживаются generic-параметрами, `.build()` без них не компилируется; версия 0.23.2. Проверено по docs.rs 2026-08-18.
- [`static_assertions` crate docs](https://docs.rs/static_assertions/) — `const_assert!`, `assert_impl_all!`, `assert_fields!` и другие проверки при компиляции; версия 1.1.0. Проверено по docs.rs 2026-08-18.
- [`rust-fsm` crate docs](https://docs.rs/rust-fsm/) — DSL `state_machine!`: состояния, входы и переходы одной декларацией; состояние — рантайм, недопустимый переход — `Err` из `consume`; версия 0.8.0 (июль 2025), 1.2M загрузок. Проверено по crates.io и docs.rs 2026-08-18.
- [`typestate` crate docs](https://docs.rs/typestate/) — proc-макрос DSL для typestate; версия 0.8.0 (июль 2021, с тех пор релизов нет), 42k загрузок. Проверено по crates.io 2026-08-18.
- [`cqrs-es` crate docs](https://docs.rs/cqrs-es/) — CQRS/event sourcing: `Aggregate` с `Command`/`Event`/`Error`, `handle` с `Result`, `apply` без; версия 0.5.0 (декабрь 2025), 162k загрузок. Проверено по crates.io и docs.rs 2026-08-18.

## Часть 4 — Nightly (generic const exprs, const traits, gen blocks, pattern types, never type)

- [A grand vision for Rust — effects](https://blog.yoshuawuyts.com/a-grand-vision-for-rust/#effects) — Yosh Wuyts. О направлении языка в сторону алгебраических эффектов.
- [Extending Rust's Effect System](https://blog.yoshuawuyts.com/extending-rusts-effect-system/) — Yosh Wuyts. Прямое продолжение «grand vision», про эффект-полиморфизм («effect generics»).
- [Coroutines, async and iter](https://without.boats/blog/coroutines-async-and-iter/) — without.boats. Связь корутин, `gen`-блоков и async.
- [RFC 3513 — gen blocks](https://github.com/rust-lang/rfcs/pull/3513) — принят 07.04.2024, резервирует `gen` в Rust 2024. Страница PR проверена 2026-10-03.
- [`Iterator::scan`](https://doc.rust-lang.org/std/iter/trait.Iterator.html#method.scan) — контракт замыкания, возвращающего `Option`: `None` означает конец итерации, а не пропуск входного элемента. Проверено 2026-10-03.
- [MCP pattern types (types-team #126)](https://github.com/rust-lang/types-team/issues/126) — Major Change Proposal по pattern types; формат MCP, а не RFC. Проверено 2026-10-03.
- [Tracking issue: pattern types, #123646](https://github.com/rust-lang/rust/issues/123646) — история реализации экспериментальной фичи. Проверено 2026-10-03.
- [Tracking issue: const traits](https://github.com/rust-lang/rust/issues/67792) — `const_trait_impl`. Проверено 2026-10-03. Константность трейтов `std` вынесена в отдельные фичи: [`const_cmp`, #143800](https://github.com/rust-lang/rust/issues/143800) и [`const_ops`, #143802](https://github.com/rust-lang/rust/issues/143802); номера взяты из сообщений компилятора nightly-2026-08-31.
- [Tracking issue: `generic_const_exprs`, #76560](https://github.com/rust-lang/rust/issues/76560) — проверено 2026-10-03; фича помечена `incomplete_features` в компиляторе. Несовместимость с новым trait solver — [#160895](https://github.com/rust-lang/rust/issues/160895), номер из предупреждения компилятора.
- [`min_generic_const_args`](https://doc.rust-lang.org/unstable-book/language-features/min-generic-const-args.html) — отдельное подмножество const-аргументов для стабилизации, без произвольных выражений в аргументе. Проверено 2026-10-03.
- [Stabilize never type — PR rust-lang/rust#155499](https://github.com/rust-lang/rust/pull/155499) — влит 24.08.2026, milestone 1.100; `Infallible` становится псевдонимом `!`. Страница PR проверена 2026-10-03.
- [`Infallible` в nightly-документации](https://doc.rust-lang.org/nightly/std/convert/type.Infallible.html) — псевдоним `!`, рекомендация использовать never type напрямую. Проверено 2026-10-03.
- [Project goal 2026: stabilize never type](https://goals.rust-lang.org/2026/stabilize-never-type.html) — цель проекта и история попыток. Найдена поиском 2026-09-13.
- [I stabilized never type](https://blog.ihatereality.space/0C-never-type/) — пост автора PR #155499; оттуда счёт «пять неудачных попыток». Повторно проверено 2026-10-03.
- [Stabilize never_type *again* — issue #57012](https://github.com/rust-lang/rust/issues/57012) — откат стабилизации для 1.41: PR #65355 и revert #67224, причина — never-type fallback. Найден поиском 2026-09-13.
- [Tracking issue: specialization (RFC 1210), #31844](https://github.com/rust-lang/rust/issues/31844) — открыт в феврале 2016, фича не стабилизирована; для вступления. Найден поиском 2026-09-13.
- [Rocket v0.5: stable, async, …](https://rocket.rs/news/2023-11-17-version-0.5/) — Rocket на stable с 0.5.0-rc.1 (09.06.2021), до этого требовал nightly с 2016 года; в статью не вошло, оставлено на случай, если вступление вернётся к этому примеру.
