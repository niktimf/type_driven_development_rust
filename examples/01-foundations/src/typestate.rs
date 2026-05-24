//! Раздел статьи «Typestate» — состояние объекта закодировано в его типе.
//!
//! Машина состояний на примере двухфакторной авторизации:
//!
//! ```text
//! AwaitingCredentials --submit_credentials--> AwaitingSecondFactor
//!                                              |
//!                                              | submit_totp
//!                                              v
//!                                          Authenticated --create_session--> Session
//! ```
//!
//! Каждый переход потребляет `self` (`fn ... (self, ...)`), и старое состояние
//! после перехода физически уничтожено. Вызвать метод не того состояния
//! не получится — он отсутствует в `impl`-блоке.

use crate::newtype::ids::UserId;

#[derive(Debug, PartialEq, Eq)]
pub enum AuthError {
    UserNotFound,
    InvalidCredentials,
    InvalidTotp,
}

pub struct Session {
    user_id: UserId,
}

impl Session {
    fn new(user_id: UserId) -> Self {
        Self { user_id }
    }

    pub fn user_id(&self) -> UserId {
        self.user_id
    }
}

/// Начальное состояние логина — никаких данных ещё нет.
///
/// Compile-fail: метод не своего состояния не вызвать.
///
/// ```compile_fail
/// use tdd_01_foundations::typestate::AwaitingCredentials;
///
/// let attempt = AwaitingCredentials::new();
/// attempt.create_session(); // no method `create_session` on AwaitingCredentials
/// ```
///
/// ```compile_fail
/// use tdd_01_foundations::typestate::AwaitingCredentials;
///
/// let attempt = AwaitingCredentials::new();
/// attempt.submit_totp("123456"); // no method `submit_totp` on AwaitingCredentials
/// ```
///
/// ```compile_fail
/// use tdd_01_foundations::typestate::AwaitingCredentials;
///
/// let attempt = AwaitingCredentials::new();
/// let _first = attempt.submit_credentials("alice", "correct");
/// let _second = attempt.submit_credentials("alice", "correct"); // attempt moved
/// ```
pub struct AwaitingCredentials;

impl AwaitingCredentials {
    pub fn new() -> Self {
        Self
    }

    /// Проверка пары username/password. В реальном коде здесь поход в БД
    /// и `Password::verify`. Здесь — упрощённо: знаем одного пользователя.
    pub fn submit_credentials(
        self,
        username: &str,
        password: &str,
    ) -> Result<AwaitingSecondFactor, AuthError> {
        if username != "alice" {
            return Err(AuthError::UserNotFound);
        }
        if password != "correct" {
            return Err(AuthError::InvalidCredentials);
        }
        Ok(AwaitingSecondFactor {
            user_id: UserId(42),
        })
    }
}

impl Default for AwaitingCredentials {
    fn default() -> Self {
        Self::new()
    }
}

/// Пароль уже прошёл. Ждём второй фактор.
pub struct AwaitingSecondFactor {
    user_id: UserId,
}

impl AwaitingSecondFactor {
    pub fn submit_totp(self, code: &str) -> Result<Authenticated, AuthError> {
        if code != "123456" {
            return Err(AuthError::InvalidTotp);
        }
        Ok(Authenticated {
            user_id: self.user_id,
        })
    }
}

/// Оба фактора прошли. Можно создавать сессию.
pub struct Authenticated {
    user_id: UserId,
}

impl Authenticated {
    pub fn user_id(&self) -> UserId {
        self.user_id
    }

    pub fn create_session(self) -> Session {
        Session::new(self.user_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn successful_two_factor_flow() {
        let attempt = AwaitingCredentials::new();
        let in_progress = attempt
            .submit_credentials("alice", "correct")
            .expect("valid creds");
        let authenticated = in_progress.submit_totp("123456").expect("valid totp");
        let session = authenticated.create_session();
        assert_eq!(session.user_id(), UserId(42));
    }

    #[test]
    fn unknown_user_returns_user_not_found() {
        let attempt = AwaitingCredentials::new();
        let result = attempt.submit_credentials("bob", "correct");
        assert_eq!(result.err(), Some(AuthError::UserNotFound));
    }

    #[test]
    fn wrong_password_returns_invalid_credentials() {
        let attempt = AwaitingCredentials::new();
        let result = attempt.submit_credentials("alice", "wrong");
        assert_eq!(result.err(), Some(AuthError::InvalidCredentials));
    }

    #[test]
    fn wrong_totp_returns_invalid_totp() {
        let in_progress = AwaitingCredentials::new()
            .submit_credentials("alice", "correct")
            .expect("valid creds");
        let result = in_progress.submit_totp("000000");
        assert_eq!(result.err(), Some(AuthError::InvalidTotp));
    }

    #[test]
    fn authenticated_exposes_user_id() {
        let authenticated = AwaitingCredentials::new()
            .submit_credentials("alice", "correct")
            .unwrap()
            .submit_totp("123456")
            .unwrap();
        assert_eq!(authenticated.user_id(), UserId(42));
    }
}
