//! Раздел статьи «ADT (алгебраические типы данных)».
//!
//! - [`Role`] — enum без данных в вариантах (просто метки).
//! - [`Token`] — enum с tuple-полями.
//! - [`FileMode`] — enum, заменяющий пару `(bool, bool)` с одним невалидным сочетанием.
//! - [`AuthOutcome`] — enum со смешанными формами вариантов; есть методы и `Display`.
//! - [`AuthEvent`] — вложенный ADT (внутри одного варианта — другой enum).

use std::fmt;
use std::net::IpAddr;
use std::time::{Duration, Instant};

use crate::newtype::ids::UserId;

/// Сессионный токен — отдельный newtype, чтобы не путать его с другими строками.
#[derive(Clone, Debug)]
pub struct SessionToken(pub String);

/// Enum без данных в вариантах — каждый вариант это просто метка.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Admin,
    User,
    Guest,
}

/// Enum с tuple-полями — каждое значение несёт `String`, но варианты различимы.
#[derive(Debug, Clone)]
pub enum Token {
    Bearer(String),
    ApiKey(String),
    Jwt(String),
}

/// Замена двум независимым `bool`-ам `(read, write)`. Из четырёх сочетаний
/// валидны только три, и в enum-е невалидного просто нет.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileMode {
    Read,
    Write,
    ReadWrite,
}

/// Enum, в котором сочетаются разные формы вариантов:
/// - `InvalidCredentials` — без данных;
/// - `RateLimited { retry_after }` — с одним именованным полем;
/// - `Success { user_id, session }` — с несколькими именованными полями.
///
/// Невозможные сочетания (например, «Success без сессии») в типе не выражаются.
///
/// ```compile_fail
/// use tdd_01_foundations::adt::AuthOutcome;
///
/// fn describe(outcome: &AuthOutcome) -> &'static str {
///     match outcome {
///         AuthOutcome::Success { .. } => "ok",
///         AuthOutcome::InvalidCredentials => "invalid",
///         // не покрыли остальные варианты — error[E0004]: non-exhaustive patterns
///     }
/// }
/// ```
#[derive(Debug, Clone)]
pub enum AuthOutcome {
    Success {
        user_id: UserId,
        session: SessionToken,
    },
    InvalidCredentials,
    AccountLocked {
        until: Instant,
    },
    RateLimited {
        retry_after: Duration,
    },
    PasswordExpired {
        user_id: UserId,
    },
}

impl AuthOutcome {
    /// Самый частый запрос к `AuthOutcome` — «получилось ли войти?».
    pub fn is_success(&self) -> bool {
        matches!(self, AuthOutcome::Success { .. })
    }

    /// Достать `UserId`, если он есть в текущем варианте. Wildcard `_` подходит,
    /// когда от остальных вариантов в этой ветке ничего не нужно.
    pub fn user_id(&self) -> Option<UserId> {
        match self {
            AuthOutcome::Success { user_id, .. } => Some(*user_id),
            AuthOutcome::PasswordExpired { user_id } => Some(*user_id),
            _ => None,
        }
    }
}

impl fmt::Display for AuthOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AuthOutcome::Success { .. } => write!(f, "ok"),
            AuthOutcome::InvalidCredentials => write!(f, "invalid credentials"),
            AuthOutcome::AccountLocked { .. } => write!(f, "account locked"),
            AuthOutcome::RateLimited { .. } => write!(f, "rate limited"),
            AuthOutcome::PasswordExpired { .. } => write!(f, "password expired"),
        }
    }
}

/// Вложенный ADT: `AuthEvent::Attempt` содержит внутри другой enum (`AuthOutcome`).
///
/// `match` на `AuthEvent` обязан покрыть все варианты, и внутри `AuthEvent::Attempt`
/// `match` на `AuthOutcome` тоже должен быть исчерпывающим. Exhaustive check работает
/// на любой глубине вложенности.
#[derive(Debug, Clone)]
pub enum AuthEvent {
    Attempt {
        outcome: AuthOutcome,
        ip: IpAddr,
        at: Instant,
    },
    Logout {
        user_id: UserId,
        at: Instant,
    },
    PasswordChanged {
        user_id: UserId,
        at: Instant,
    },
}

/// Краткое строковое описание исхода логина — демонстрация `match` по `AuthOutcome`.
pub fn describe_outcome(outcome: &AuthOutcome) -> &'static str {
    match outcome {
        AuthOutcome::Success { .. } => "ok",
        AuthOutcome::InvalidCredentials => "invalid",
        AuthOutcome::AccountLocked { .. } => "locked",
        AuthOutcome::RateLimited { .. } => "throttled",
        AuthOutcome::PasswordExpired { .. } => "expired",
    }
}

/// Двухуровневый `match`: сначала по `AuthEvent`, внутри `Attempt` — по `AuthOutcome`.
pub fn describe_event(event: &AuthEvent) -> &'static str {
    match event {
        AuthEvent::Attempt { outcome, .. } => match outcome {
            AuthOutcome::Success { .. } => "attempt: ok",
            AuthOutcome::InvalidCredentials => "attempt: invalid",
            AuthOutcome::AccountLocked { .. } => "attempt: locked",
            AuthOutcome::RateLimited { .. } => "attempt: throttled",
            AuthOutcome::PasswordExpired { .. } => "attempt: expired",
        },
        AuthEvent::Logout { .. } => "logout",
        AuthEvent::PasswordChanged { .. } => "password changed",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    fn sample_outcomes() -> Vec<AuthOutcome> {
        vec![
            AuthOutcome::Success {
                user_id: UserId(1),
                session: SessionToken("session-token".into()),
            },
            AuthOutcome::InvalidCredentials,
            AuthOutcome::AccountLocked {
                until: Instant::now(),
            },
            AuthOutcome::RateLimited {
                retry_after: Duration::from_secs(30),
            },
            AuthOutcome::PasswordExpired {
                user_id: UserId(1),
            },
        ]
    }

    #[test]
    fn each_outcome_describes_to_unique_string() {
        let descriptions: Vec<_> = sample_outcomes().iter().map(describe_outcome).collect();
        assert_eq!(
            descriptions,
            ["ok", "invalid", "locked", "throttled", "expired"]
        );
    }

    #[test]
    fn role_variants_compare_by_value() {
        assert_eq!(Role::Admin, Role::Admin);
        assert_ne!(Role::Admin, Role::User);
        assert_ne!(Role::User, Role::Guest);
    }

    #[test]
    fn tuple_variants_carry_their_payload() {
        let tokens = [
            Token::Bearer("abc".into()),
            Token::ApiKey("xyz".into()),
            Token::Jwt("eyJ...".into()),
        ];

        // Каждый вариант различим в match, payload доступен.
        let kinds: Vec<_> = tokens
            .iter()
            .map(|t| match t {
                Token::Bearer(_) => "bearer",
                Token::ApiKey(_) => "api-key",
                Token::Jwt(_) => "jwt",
            })
            .collect();

        assert_eq!(kinds, ["bearer", "api-key", "jwt"]);
    }

    #[test]
    fn file_mode_has_no_invalid_state() {
        // Демонстрация: четвёртое сочетание (false, false) в типе просто отсутствует.
        let valid_modes = [FileMode::Read, FileMode::Write, FileMode::ReadWrite];
        assert_eq!(valid_modes.len(), 3);
    }

    #[test]
    fn is_success_matches_only_success_variant() {
        let success = AuthOutcome::Success {
            user_id: UserId(1),
            session: SessionToken("s".into()),
        };
        let invalid = AuthOutcome::InvalidCredentials;
        let expired = AuthOutcome::PasswordExpired { user_id: UserId(1) };

        assert!(success.is_success());
        assert!(!invalid.is_success());
        assert!(!expired.is_success());
    }

    #[test]
    fn user_id_returns_for_variants_that_carry_it() {
        let success = AuthOutcome::Success {
            user_id: UserId(42),
            session: SessionToken("s".into()),
        };
        let expired = AuthOutcome::PasswordExpired { user_id: UserId(42) };
        let invalid = AuthOutcome::InvalidCredentials;
        let locked = AuthOutcome::AccountLocked {
            until: Instant::now(),
        };

        assert_eq!(success.user_id(), Some(UserId(42)));
        assert_eq!(expired.user_id(), Some(UserId(42)));
        assert_eq!(invalid.user_id(), None);
        assert_eq!(locked.user_id(), None);
    }

    #[test]
    fn display_format_for_each_variant() {
        let descriptions: Vec<_> = sample_outcomes()
            .iter()
            .map(|o| format!("{o}"))
            .collect();

        assert_eq!(
            descriptions,
            [
                "ok",
                "invalid credentials",
                "account locked",
                "rate limited",
                "password expired",
            ]
        );
    }

    #[test]
    fn nested_match_dispatches_through_both_layers() {
        let ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));

        let events = vec![
            AuthEvent::Attempt {
                outcome: AuthOutcome::Success {
                    user_id: UserId(1),
                    session: SessionToken("s".into()),
                },
                ip,
                at: Instant::now(),
            },
            AuthEvent::Attempt {
                outcome: AuthOutcome::InvalidCredentials,
                ip,
                at: Instant::now(),
            },
            AuthEvent::Logout {
                user_id: UserId(1),
                at: Instant::now(),
            },
            AuthEvent::PasswordChanged {
                user_id: UserId(1),
                at: Instant::now(),
            },
        ];

        let descriptions: Vec<_> = events.iter().map(describe_event).collect();
        assert_eq!(
            descriptions,
            [
                "attempt: ok",
                "attempt: invalid",
                "logout",
                "password changed"
            ]
        );
    }
}
