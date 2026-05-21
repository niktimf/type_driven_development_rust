# Type-driven development в Rust

Серия из 4 технических статей для Habr и компилируемые примеры кода к ним.

## Статьи

| #   | Часть                                       | Статус        | Аннотация                                                       |
|-----|---------------------------------------------|---------------|-----------------------------------------------------------------|
| 1/4 | [Основы](articles/01-foundations.md)        | in progress   | newtype, ADT, пустые типы, phantom types, typestate             |
| 2/4 | [Контракты](articles/02-contracts.md)       | planned       | трейты, ассоциированные типы, const generics                    |
| 3/4 | [Валидатор](articles/03-validator.md)       | planned       | типы-списки, compile-time валидаторы, event sourcing            |
| 4/4 | [Nightly](articles/04-nightly.md)           | planned       | pattern types, const traits, gen-блоки                          |

## Примеры кода

В каталоге `examples/` лежит Cargo workspace с четырьмя крейтами — по одному на каждую часть серии. Стабильные крейты собираются из корня воркспейса:

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