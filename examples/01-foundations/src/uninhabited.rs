//! Раздел статьи «Uninhabited types» (типы без значений) — на биржевом домене.
//!
//! Главный инструмент здесь — [`std::convert::Infallible`]: пустой enum, в котором
//! значений не существует. Если в `Result<T, Infallible>` ветка `Err` невозможна,
//! обрабатывать её можно «пустым» match, без unwrap и unreachable.

use std::convert::Infallible;
use std::str::FromStr;

/// Анти-пример из статьи: 
/// свободной функции `Result<i32, Infallible>` не нужен — он оправдан,
/// только когда нужно соответствовать `Result`-форме трейта.
pub fn always_ok_wrapped() -> Result<i32, Infallible> {
    Ok(24)
}

/// Правильно: функция, которой не нужно сидеть в `Result`-интерфейсе, просто возвращает `T`.
pub fn always_ok() -> i32 {
    24
}

/// Клиентский идентификатор заявки (в FIX — ClOrdID): произвольная строка, которую задаёт сам клиент.
/// Принимаем что угодно, ошибиться в `from_str` нельзя — поэтому `type Err = Infallible`.
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
    fn parse_unwraps_via_empty_match() {
        // Разворачивание из статьи: без unwrap-а и без unreachable.
        let id: ClientOrderId = match "order-2026-0001".parse::<ClientOrderId>() {
            Ok(id) => id,
            Err(never) => match never {}, // пустой match — веток ноль
        };
        assert_eq!(id.0, "order-2026-0001");
    }

    #[test]
    fn always_ok_pair_behaves_identically() {
        assert_eq!(always_ok(), 24);
        let unwrapped = match always_ok_wrapped() {
            Ok(v) => v,
            Err(never) => match never {},
        };
        assert_eq!(unwrapped, 24);
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
