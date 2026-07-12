# Type-driven development в Rust

Серия из 5 технических статей для Habr и компилируемые примеры кода к ним.

## Статьи

| #   | Часть                                 | Статус      | Аннотация                                                   |
|-----|---------------------------------------|-------------|-------------------------------------------------------------|
| 1/5 | [Основы](articles/01-foundations.md)  | ready       | newtype, ADT, uninhabited types, phantom types, typestate   |
| 2/5 | [Контракты](articles/02-contracts.md) | in progress | traits, associated types, const generics                    |
| 3/5 | [Валидатор](articles/03-validator.md) | planned     | type-level lists, compile-time validators, event sourcing   |
| 4/5 | [Nightly](articles/04-nightly.md)     | planned     | pattern types, const traits, gen blocks, never type         |
| 5/5 | [Будущее](articles/05-future.md)      | planned     | substructural types, effects, variadic generics, view types |

## Примеры кода

В каталоге `examples/` лежит Cargo workspace с четырьмя крейтами — по одному на части 1–4 (у Части 5 кода пока нет). Стабильные крейты собираются из корня воркспейса:

```sh
cd examples
cargo build --workspace --exclude tdd-04-nightly
cargo test  --workspace --exclude tdd-04-nightly
```

Крейт `04-nightly` требует nightly-toolchain. Он зафиксирован в `examples/04-nightly/rust-toolchain.toml`, и подхватывается только когда `cargo` запускается из каталога самого крейта (так устроен поиск `rust-toolchain.toml` — вверх от cwd):

```sh
cd examples/04-nightly
cargo build
cargo test
```
