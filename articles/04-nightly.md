# Type-driven development в Rust. Часть 4/5: выходим за стабильный Rust — pattern types, const traits, gen blocks, never type

_Черновик в работе._

Планируется:
- Pattern types (RFC 3513, simple form of refinement types)
- Const traits
- Gen-блоки (generators)
- Never type (`!`) в позициях типа — стабилизирован в Rust 1.100 (компилятор 1.100.0-nightly
от 2026-08-31 собирает `type Err = !` и `Result<T, !>` без feature-флага).
Часть 1 отправляла за ним сюда, поэтому раздел остаётся, но подаётся как «что успело
стабилизироваться»: `ClientOrderId` из части 1 с `type Err = !` вместо `Infallible`,
`Err(never) => never` вместо пустого `match`; `Infallible` остаётся для MSRV ниже 1.100.