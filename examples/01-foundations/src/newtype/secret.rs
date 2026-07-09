//! Анти-пример из статьи: `Deref` на секретной обёртке — тихий обход маскировки.
//!
//! Маскирующий `Debug` закрывает только `{:?}`, но против `Deref` бессилен:
//! `&ApiKey` сам сходит за `&str` и утечёт в любой строковый API без единого
//! видимого преобразования. Поэтому `Deref` оставляют прозрачным обёрткам
//! (`Box<T>`, `Rc<T>`), а для newtype с инвариантом или секретом — только явные
//! геттеры. Готовая альтернатива — крейт `secrecy` (`SecretString`).

use std::fmt;
use std::ops::Deref;

/// API-ключ биржи, которому нельзя попадать в логи.
pub struct ApiKey(pub String);

impl fmt::Debug for ApiKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ApiKey(\"***\")") // маскируем
    }
}

impl Deref for ApiKey {
    // <- вот эта реализация всё и ломает; оставлена нарочно, как в статье
    type Target = str;
    fn deref(&self) -> &str {
        &self.0
    }
}

/// Строковый API, куда секрет утекает через deref coercion.
pub fn log(line: &str) {
    let _ = line; /* пишет в файл */
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_masks_the_secret() {
        let key = ApiKey("sk-secret".to_string());
        assert_eq!(format!("{key:?}"), "ApiKey(\"***\")");
    }

    #[test]
    fn deref_leaks_past_the_masked_debug() {
        let key = ApiKey("sk-secret".to_string());
        // Компилируется: &ApiKey молча превратился в &str — ни .expose(),
        // ни явной конверсии. Это и есть утечка из статьи.
        log(&key);
        let leaked: &str = &key;
        assert_eq!(leaked, "sk-secret");
    }
}