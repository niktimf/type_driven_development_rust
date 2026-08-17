//! Примеры к части 4/5: pattern types, const traits, gen blocks, never type.
//!
//! Крейт требует nightly: см. `rust-toolchain.toml` рядом. Feature-gate'ы
//! будут добавляться по мере раскрытия тем в статье.
//!
//! Зависит от `tdd_02_contracts`: nightly-варианты показываются на тех же
//! типах, что и стабильные обходы в части 2.

#![feature(never_type)]
#![feature(generic_const_exprs)]
#![allow(incomplete_features)]

pub mod const_bounds;

/// Заглушка: расходящаяся функция, иллюстрирующая stable `!` через nightly-фичу.
/// Будет заменена реальным примером в статье.
pub fn diverges() -> ! {
    todo!("заполнить в статье — раздел про пустые типы и pattern types")
}

#[cfg(test)]
mod tests {
    #[test]
    fn skeleton_compiles() {}
}
