//! Pattern types: диапазон записан в самом типе значения, без обёртки.
//!
//! Фича помечена внутренней, поэтому в `lib.rs` стоит `#![allow(internal_features)]`.
//! В безопасном примере значение задаётся подходящим литералом; преобразование
//! рантайм-значения показано через `transmute` после проверки, см. [`depth_from_config`].
//! Для ненулевой глубины на stable уже доступен `NonZeroUsize::new`.

use std::pat::pattern_type;

/// Ненулевая глубина как значение. Этот тип не задаёт длину массива
/// и не заменяет `OrderBook<DEPTH>` с ограничением `SupportedDepth` из части 2.
pub type Depth = pattern_type!(usize is 1..);

/// Снимок стакана: глубина не бывает нулём.
///
/// Литерал в диапазоне проходит:
///
/// ```
/// #![feature(pattern_types, pattern_type_macro)]
/// #![allow(internal_features)]
/// use tdd_04_nightly::pattern_types::Snapshot;
///
/// let s = Snapshot { depth: 10 };
/// ```
///
/// Литерал вне диапазона отклоняется при компиляции:
///
/// ```compile_fail,E0308
/// #![feature(pattern_types, pattern_type_macro)]
/// #![allow(internal_features)]
/// use tdd_04_nightly::pattern_types::Snapshot;
///
/// // error[E0308]: mismatched types
/// //    expected `pattern_type!(usize is 1..)`, found integer
/// let s = Snapshot { depth: 0 };
/// ```
///
/// Обычный `usize`, даже с подходящим значением, присвоить нельзя:
///
/// ```compile_fail,E0308
/// #![feature(pattern_types, pattern_type_macro)]
/// #![allow(internal_features)]
/// use tdd_04_nightly::pattern_types::Snapshot;
///
/// // error[E0308]: expected `pattern_type!(usize is 1..)`, found `usize`
/// let n: usize = 10;
/// let s = Snapshot { depth: n };
/// ```
///
/// И обратно: `s.depth` не присвоить в `usize`:
///
/// ```compile_fail,E0308
/// #![feature(pattern_types, pattern_type_macro)]
/// #![allow(internal_features)]
/// use tdd_04_nightly::pattern_types::Snapshot;
///
/// let s = Snapshot { depth: 10 };
/// // error[E0308]: mismatched types
/// let n: usize = s.depth;
/// ```
pub struct Snapshot {
    pub depth: Depth,
}

/// Преобразование рантайм-значения через `transmute` после своей проверки.
/// Проверка диапазона при этом остаётся на программисте, компилятор её не делает.
pub fn depth_from_config(n: usize) -> Option<Depth> {
    if n >= 1 {
        // SAFETY: проверка `n >= 1` выше — ровно паттерн `1..`, представление совпадает с `usize`.
        Some(unsafe { std::mem::transmute::<usize, Depth>(n) })
    } else {
        None
    }
}

/// Паттерны с диапазоном поддерживают только целочисленные типы и `char`, поэтому `Price` на `Decimal`
/// таким способом не описать:
///
/// ```compile_fail,E0277
/// #![feature(pattern_types, pattern_type_macro)]
/// #![allow(internal_features)]
/// use std::pat::pattern_type;
///
/// // error[E0277]: `f64` is not a valid base type for range patterns
/// //    only integer types and `char` are supported
/// type Bad = pattern_type!(f64 is 1.0..);
/// ```
///
/// Синтаксис без макроса, `usize is 1..`, на nightly от 2026-08-31 не парсится:
///
/// ```compile_fail
/// #![feature(pattern_types, pattern_type_macro)]
/// #![allow(internal_features)]
///
/// type Depth = usize is 1..;
/// ```
pub const fn base_types_are_integers_and_char() {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::size_of;

    #[test]
    fn zero_is_used_for_none() {
        assert_eq!(size_of::<Option<Depth>>(), size_of::<usize>());
        assert_eq!(size_of::<Option<usize>>(), 2 * size_of::<usize>());
    }

    #[test]
    fn runtime_value_needs_a_check_and_transmute() {
        assert!(depth_from_config(0).is_none());
        let depth = depth_from_config(10).unwrap();
        // SAFETY: обратное преобразование в базовый тип всегда корректно.
        let back: usize = unsafe { std::mem::transmute::<Depth, usize>(depth) };
        assert_eq!(back, 10);
    }
}
