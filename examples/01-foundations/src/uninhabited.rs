//! Раздел статьи «Uninhabited types» (типы без значений) — на биржевом домене.
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

/// Клиентский идентификатор заявки (в FIX — clOrdID): произвольная строка, которую
/// задаёт сам клиент. Принимаем что угодно, ошибиться в `from_str` физически нельзя —
/// поэтому `type Err = Infallible`.
pub struct ClientOrderId(pub String);

impl FromStr for ClientOrderId {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(ClientOrderId(s.to_string()))
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
    fn client_order_id_from_str_is_infallible() {
        // `.parse()` для ClientOrderId возвращает Result<_, Infallible>.
        // Err невозможна — unwrap по построению не упадёт.
        let id: ClientOrderId = "order-2026-0001".parse().unwrap();
        assert_eq!(id.0, "order-2026-0001");
    }

    #[test]
    fn client_order_id_accepts_arbitrary_input() {
        // Принимаем что угодно: пустая строка, юникод, эмодзи.
        for raw in ["", "abc-123", "Заявка", "🦀", "  spaces  "] {
            let id: ClientOrderId = raw.parse().unwrap();
            assert_eq!(id.0, raw);
        }
    }
}
