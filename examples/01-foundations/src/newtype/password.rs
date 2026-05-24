//! Newtype с инвариантом: `Password` вокруг argon2-хеша.
//!
//! - Поле приватно, plaintext-конструктор отсутствует.
//! - `Password::hash` — единственный путь создания «с нуля».
//! - `Password::from_hash` — валидирующая реконструкция из недоверенного хранилища.
//! - `Password::from_hash_unchecked` — невалидирующая, для строго доверенного.
//! - `Debug` маскирует значение.
//! - `Deref` НЕ реализован сознательно (см. ниже).

use std::fmt;

use argon2::{
    Algorithm, Argon2, Params, Version,
    password_hash::{
        PasswordHash, PasswordHasher, PasswordVerifier, SaltString,
        rand_core::OsRng,
    },
};

/// Newtype вокруг argon2-хеша пароля.
///
/// Пример из статьи: без `Deref<Target = str>` deref coercion `&Password` → `&str`
/// не работает, и хеш нельзя передать в функцию, ожидающую `&str`:
///
/// ```compile_fail
/// use tdd_01_foundations::newtype::password::Password;
///
/// fn write_audit(_action: &str) {}
///
/// let password = Password::hash("secret123").unwrap();
/// write_audit(&password); // expected `&str`, found `&Password`
/// ```
#[derive(Clone, PartialEq, Eq)]
pub struct Password(String);

impl Password {
    // OWASP-рекомендуемые параметры (2023).
    // https://cheatsheetseries.owasp.org/cheatsheets/Password_Storage_Cheat_Sheet.html
    const ARGON2_MEMORY_KIB: u32 = 65536;
    const ARGON2_ITERATIONS: u32 = 3;
    const ARGON2_LANES: u32 = 4;
    const ARGON2_OUTPUT_LEN: usize = 32;

    pub fn hash<S: AsRef<str>>(input: S) -> Result<Self, argon2::password_hash::Error> {
        let params = Params::new(
            Self::ARGON2_MEMORY_KIB,
            Self::ARGON2_ITERATIONS,
            Self::ARGON2_LANES,
            Some(Self::ARGON2_OUTPUT_LEN),
        )?;

        let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
        let salt = SaltString::generate(&mut OsRng);

        let hash = argon2.hash_password(input.as_ref().as_bytes(), &salt)?;
        Ok(Self(hash.to_string()))
    }

    pub fn verify(&self, password: &str) -> bool {
        PasswordHash::new(&self.0)
            .map(|hash| {
                Argon2::default()
                    .verify_password(password.as_bytes(), &hash)
                    .is_ok()
            })
            .unwrap_or(false)
    }

    pub fn from_hash(hash: String) -> Result<Self, argon2::password_hash::Error> {
        PasswordHash::new(&hash)?; // парсим PHC-формат; если не валиден — Err
        Ok(Self(hash))
    }

    pub const fn from_hash_unchecked(hash: String) -> Self {
        Self(hash)
    }
}

impl fmt::Debug for Password {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Password(\"********\")")
    }
}

impl AsRef<str> for Password {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_and_verify_roundtrip() {
        let password = Password::hash("secret123").unwrap();
        assert!(password.verify("secret123"));
        assert!(!password.verify("wrong_password"));
    }

    #[test]
    fn debug_masks_the_hash() {
        let password = Password::hash("secret123").unwrap();
        let debug = format!("{password:?}");
        assert_eq!(debug, "Password(\"********\")");
    }

    #[test]
    fn as_ref_returns_a_phc_string() {
        let password = Password::hash("x").unwrap();
        let stored: &str = password.as_ref();
        assert!(stored.starts_with("$argon2id$"));
    }

    #[test]
    fn from_hash_roundtrips_with_verify() {
        // Сценарий: хеш уже посчитан и лежит в БД, грузим обратно через валидирующий путь.
        let original = Password::hash("secret123").unwrap();
        let stored = original.as_ref().to_string();

        let loaded = Password::from_hash(stored).unwrap();
        assert!(loaded.verify("secret123"));
        assert!(!loaded.verify("wrong"));
    }

    #[test]
    fn from_hash_rejects_garbage() {
        // Валидирующий from_hash отсеивает не-PHC прямо на входе.
        let result = Password::from_hash("not-a-valid-phc-string".to_string());
        assert!(result.is_err());
    }

    #[test]
    fn from_hash_unchecked_with_garbage_silently_returns_false() {
        // Невалидирующий путь — мусор просто попадает внутрь Password.
        // verify падает на парсинге и возвращает false, паники нет.
        let bogus = Password::from_hash_unchecked("not-a-valid-phc-string".to_string());
        assert!(!bogus.verify("any"));
    }

    #[test]
    fn from_hash_unchecked_roundtrips_with_verify() {
        // Невалидирующий путь работает корректно, если хеш всё-таки валидный.
        let original = Password::hash("secret123").unwrap();
        let stored = original.as_ref().to_string();

        let loaded = Password::from_hash_unchecked(stored);
        assert!(loaded.verify("secret123"));
    }

    #[test]
    fn two_hashes_of_the_same_password_differ() {
        // Соль случайная — каждый раз должен получаться разный хеш.
        let a = Password::hash("secret123").unwrap();
        let b = Password::hash("secret123").unwrap();
        assert_ne!(a.as_ref(), b.as_ref());
        // Но обе версии валидируются одним и тем же plaintext.
        assert!(a.verify("secret123"));
        assert!(b.verify("secret123"));
    }
}
