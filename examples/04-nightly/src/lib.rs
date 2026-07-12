//! Примеры к части 4/4: pattern types, const traits, gen-блоки.
//!
//! Крейт требует nightly: см. `rust-toolchain.toml` рядом. Feature-gate'ы
//! будут добавляться по мере раскрытия тем в статье.

#![feature(never_type)]

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
