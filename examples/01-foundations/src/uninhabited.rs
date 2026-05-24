//! Раздел статьи «Uninhabited types» (типы без значений).
//!
//! Главный инструмент здесь — [`std::convert::Infallible`]: пустой enum, в котором
//! значений не существует. Если в `Result<T, Infallible>` ветка `Err` физически
//! невозможна, обрабатывать её можно «пустым» match, без unwrap и unreachable.

use std::convert::Infallible;
use std::str::FromStr;

/// Простой пример: функция, которая по построению не может вернуть ошибку,
/// но возвращает `Result` (например, чтобы совпасть с интерфейсом другой).
pub fn always_ok() -> Result<i32, Infallible> {
    Ok(42)
}

/// Newtype-обёртка над именем пользователя. Принимает любую строку, ошибиться
/// в `from_str` физически не может — поэтому `type Err = Infallible`.
pub struct Username(pub String);

impl FromStr for Username {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Username(s.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn always_ok_returns_value_via_empty_match() {
        // Демонстрация паттерна: ветка Err формально присутствует,
        // но её тело — пустой match по Infallible, потому что значений нет.
        let value: i32 = match always_ok() {
            Ok(v) => v,
            Err(never) => match never {},
        };
        assert_eq!(value, 42);
    }

    #[test]
    fn username_from_str_is_infallible() {
        // `.parse()` для Username возвращает Result<_, Infallible>.
        // Err невозможна — unwrap по построению не упадёт.
        let name: Username = "alice".parse().unwrap();
        assert_eq!(name.0, "alice");
    }

    #[test]
    fn username_accepts_arbitrary_input() {
        // Принимаем что угодно: пустая строка, юникод, эмодзи.
        for raw in ["", "alice", "Иван", "🦀", "  spaces  "] {
            let name: Username = raw.parse().unwrap();
            assert_eq!(name.0, raw);
        }
    }
}
