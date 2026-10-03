//! Примеры к части 4/5: generic const exprs, const traits, gen blocks,
//! pattern types, never type.
//!
//! Крейт требует nightly: см. `rust-toolchain.toml` рядом.
//!
//! Модуль <-> раздел статьи:
//! - `const_bounds` — «generic_const_exprs»: условие `N <= MAX_BATCH` в bound-е;
//! - `const_limits` — «Const traits»: потолки лимитов, `<` и `*` на newtype при сборке;
//! - `gen_blocks` — «Gen-блоки»: агрегированный стакан циклом с `yield`;
//! - `pattern_types` — «Pattern types»: глубина стакана как `usize is 1..`;
//! - `never_type` — «Never type»: `type Err = !` и `Infallible` как псевдоним.
//!
//! Зависит от `tdd_02_contracts`: nightly-варианты показываются на тех же
//! типах, что и код части 2 под stable; `Money` переопределён, потому что
//! чужому типу const-реализацию трейта не дописать.

// `never_type` стабилен с 1.100: на текущем nightly атрибут даёт `stable_features`.
#![feature(generic_const_exprs)]
#![feature(const_trait_impl, const_cmp, const_ops)]
#![feature(gen_blocks)]
#![feature(pattern_types, pattern_type_macro)]
#![allow(incomplete_features, internal_features)]

pub mod const_bounds;
pub mod const_limits;
pub mod gen_blocks;
pub mod never_type;
pub mod pattern_types;
