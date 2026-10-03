//! Потолки лимитов, проверенные при сборке: сравнение и арифметика на newtype
//! в `const`. Без const traits `const fn` не вызывает методы трейтов,
//! а `<` и `*` — это `PartialOrd::lt` и `Mul::mul`.
//!
//! Лимиты из конфига часть 3 проверяет в рантайме. Потолки для них зашиты
//! в код намеренно: конфиг может опустить лимит, поднять выше потолка не может,
//! а сам потолок меняется только через ревью и деплой. Соотношения между
//! потолками («предупреждение раньше остановки») здесь проверяет компилятор.
//!
//! Шаг с `const fn`, `Result` и `match` работает на stable; nightly нужен
//! для оператора `<` в `const`.
//!
//! В `std` const traits применены не везде: `PartialOrd` у `Duration` — обычный
//! derive, и сравнение таймаутов в `const` пока не собирается:
//!
//! ```compile_fail,E0277
//! #![feature(const_trait_impl, const_cmp)]
//! use std::time::Duration;
//!
//! // error[E0277]: the trait bound `Duration: const PartialOrd` is not satisfied
//! //   note: trait `PartialOrd` is implemented but not `const`
//! const _: () = assert!(Duration::from_millis(100) < Duration::from_millis(500));
//! ```

use std::cmp::Ordering;
use std::marker::PhantomData;
use std::ops::Mul;

use rust_decimal::{Decimal, dec};
use tdd_02_contracts::domain::{Currency, Usd};

/// Деньги в валюте `C`. `Money` части 2 хранит тот же `Decimal`, но его `PartialEq` —
/// обычный derive, а чужому типу const-реализацию не дописать, поэтому тип переопределён здесь.
#[derive(Debug, Clone, Copy)]
pub struct Money<C> {
    amount: Decimal,
    _currency: PhantomData<C>,
}

/// Ошибка конструктора. `panic!` в `const` принимает только `&str` без форматирования,
/// отсюда [`MoneyError::message`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoneyError {
    /// Число знаков после запятой не совпадает с `Currency::MINOR_UNITS`.
    Scale,
}

impl MoneyError {
    pub const fn message(&self) -> &'static str {
        match self {
            Self::Scale => "масштаб не совпадает с валютой",
        }
    }
}

impl<C: Currency> Money<C> {
    /// Один конструктор на оба контекста: в рантайме возвращает `Err`,
    /// в `const` его разбирает `match` (см. [`usd`]), и `Err` становится ошибкой сборки.
    ///
    /// Масштаб фиксируется валютой: дальше сравнение и умножение работают на мантиссах.
    pub const fn new(amount: Decimal) -> Result<Self, MoneyError> {
        if amount.scale() != C::MINOR_UNITS {
            return Err(MoneyError::Scale);
        }
        Ok(Self {
            amount,
            _currency: PhantomData,
        })
    }

    pub const fn amount(&self) -> Decimal {
        self.amount
    }
}

/// Литерал суммы в долларах для `const`-контекста.
///
/// Литерал с неверным масштабом не собирается — `E0080` с текстом из `MoneyError`:
///
/// ```compile_fail,E0080
/// #![feature(const_trait_impl, const_cmp, const_ops)]
/// use rust_decimal::dec;
/// use tdd_04_nightly::const_limits::usd;
///
/// // error[E0080]: evaluation panicked: масштаб не совпадает с валютой
/// const BAD: tdd_04_nightly::const_limits::Money<tdd_02_contracts::domain::Usd> = usd(dec!(750_000.0));
/// ```
pub const fn usd(amount: Decimal) -> Money<Usd> {
    match Money::new(amount) {
        Ok(money) => money,
        Err(e) => panic!("{}", e.message()),
    }
}

/// Масштаб у всех `Money<C>` одной валюты общий — его проверил конструктор,
/// поэтому сравниваются мантиссы. Собственное сравнение `Decimal` не `const`.
///
/// Константность трейтов `std` вынесена в отдельные фичи: без `const_cmp`
/// `const impl PartialEq` не собирается даже для своего типа:
///
/// ```compile_fail,E0658
/// #![feature(const_trait_impl)]
///
/// struct Cap(i128);
/// // error[E0658]: use of unstable const library feature `const_cmp`
/// //   trait is not stable as const yet
/// const impl PartialEq for Cap { fn eq(&self, o: &Self) -> bool { self.0 == o.0 } }
/// ```
const impl<C> PartialEq for Money<C> {
    fn eq(&self, other: &Self) -> bool {
        self.amount.mantissa() == other.amount.mantissa()
    }
}

/// Реализация обязана быть `const`: обычный `impl PartialOrd` под `<` в `const`
/// не подходит, компилятор подсказывает добавить `const` перед `impl`:
///
/// ```compile_fail,E0277
/// #![feature(const_trait_impl, const_cmp)]
/// use std::cmp::Ordering;
///
/// struct Cap(i128);
/// impl PartialEq for Cap { fn eq(&self, o: &Self) -> bool { self.0 == o.0 } }
/// impl PartialOrd for Cap {
///     fn partial_cmp(&self, o: &Self) -> Option<Ordering> { self.0.partial_cmp(&o.0) }
/// }
///
/// // error[E0277]: the trait bound `Cap: const PartialOrd` is not satisfied
/// //   help: make the `impl` of trait `PartialOrd` `const`
/// const _: () = assert!(Cap(1) < Cap(2));
/// ```
const impl<C> PartialOrd for Money<C> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        let (a, b) = (self.amount.mantissa(), other.amount.mantissa());
        Some(if a < b {
            Ordering::Less
        } else if a > b {
            Ordering::Greater
        } else {
            Ordering::Equal
        })
    }
}

/// `Decimal` из мантиссы и масштаба. `from_i128_with_scale` не `const`,
/// поэтому мантисса раскладывается на три слова для `from_parts`.
const fn from_mantissa(mantissa: i128, scale: u32) -> Decimal {
    let (negative, m) = (mantissa < 0, mantissa.unsigned_abs());
    assert!(m <= (1_u128 << 96) - 1, "мантисса не помещается в Decimal");
    Decimal::from_parts(
        m as u32,
        (m >> 32) as u32,
        (m >> 64) as u32,
        negative,
        scale,
    )
}

/// Умножение на целое не меняет масштаб: мантисса умножается, `scale` остаётся.
/// Так производный потолок считается при сборке.
///
/// # Panics
///
/// При переполнении `i128` или 96-битной мантиссы `Decimal`.
/// В `const`-контексте это ошибка сборки; для внешних сумм нужен fallible-метод.
///
/// ```compile_fail,E0080
/// #![feature(const_trait_impl, const_cmp, const_ops)]
/// use rust_decimal::Decimal;
/// use tdd_02_contracts::domain::Usd;
/// use tdd_04_nightly::const_limits::Money;
///
/// const MAX: Money<Usd> = match Money::new(
///     Decimal::from_parts(u32::MAX, u32::MAX, u32::MAX, false, 2)
/// ) {
///     Ok(money) => money,
///     Err(_) => panic!("неверный масштаб"),
/// };
/// const TOO_LARGE: Money<Usd> = MAX * 2;
/// ```
const impl<C> Mul<u32> for Money<C> {
    type Output = Money<C>;

    fn mul(self, n: u32) -> Money<C> {
        let mantissa = match self.amount.mantissa().checked_mul(n as i128) {
            Some(mantissa) => mantissa,
            None => panic!("переполнение мантиссы i128"),
        };
        Money {
            amount: from_mantissa(mantissa, self.amount.scale()),
            _currency: PhantomData,
        }
    }
}

/// Ступени идут по возрастанию. Bound `[const]`: в `const` нужна константная
/// реализация, в рантайме подойдёт любая — та же функция проверяет ступени из конфига.
///
/// Bound `[const]` обязателен: с обычным `T: PartialOrd` тело в `const fn`
/// не собирается, компилятор называет недостающий bound по имени:
///
/// ```compile_fail,E0277
/// #![feature(const_trait_impl, const_cmp)]
/// // error[E0277]: the trait bound `T: [const] PartialOrd` is not satisfied
/// const fn ascending<T: PartialOrd>(steps: &[T]) -> bool {
///     let mut i = 1;
///     while i < steps.len() {
///         if !(steps[i - 1] < steps[i]) { return false; }
///         i += 1;
///     }
///     true
/// }
/// ```
pub const fn ascending<T: [const] PartialOrd>(steps: &[T]) -> bool {
    let mut i = 1;
    while i < steps.len() {
        if !(steps[i - 1] < steps[i]) {
            return false;
        }
        i += 1;
    }
    true
}

/// Потолок потерь на одной сделке.
pub const MAX_LOSS_PER_TRADE: Money<Usd> = usd(dec!(50_000.00));
/// Дневной потолок потерь — производная константа: двадцать потолков на сделку.
///
/// Перепутанные потолки не собираются. Множитель `10` даёт дневной потолок
/// `500_000.00`, ниже порога предупреждения:
///
/// ```compile_fail,E0080
/// #![feature(const_trait_impl, const_cmp, const_ops)]
/// use tdd_02_contracts::domain::Usd;
/// use tdd_04_nightly::const_limits::{Money, MAX_LOSS_PER_TRADE, WARN_DAILY_LOSS};
///
/// const MAX_DAILY_LOSS: Money<Usd> = MAX_LOSS_PER_TRADE * 10;
/// // error[E0080]: evaluation panicked: предупреждение должно срабатывать раньше остановки
/// const _: () = assert!(WARN_DAILY_LOSS < MAX_DAILY_LOSS, "предупреждение должно срабатывать раньше остановки");
/// ```
pub const MAX_DAILY_LOSS: Money<Usd> = MAX_LOSS_PER_TRADE * 20;
/// Порог предупреждения по дневным потерям.
pub const WARN_DAILY_LOSS: Money<Usd> = usd(dec!(750_000.00));

const _: () = assert!(
    WARN_DAILY_LOSS < MAX_DAILY_LOSS,
    "предупреждение должно срабатывать раньше остановки"
);
const _: () = assert!(
    ascending(&[MAX_LOSS_PER_TRADE, WARN_DAILY_LOSS, MAX_DAILY_LOSS]),
    "ступени лимитов должны расти: сделка < предупреждение < остановка"
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multiplication_preserves_sign_and_currency_scale() {
        let money = usd(dec!(-12.50));

        let result = money * 3;

        assert_eq!(result.amount(), dec!(-37.50));
        assert_eq!(result.amount().scale(), 2);
    }

    #[test]
    #[should_panic(expected = "мантисса не помещается в Decimal")]
    fn multiplication_rejects_amount_outside_decimal_range() {
        let money = usd(Decimal::from_parts(u32::MAX, u32::MAX, u32::MAX, false, 2));

        let _ = money * 2;
    }

    #[test]
    #[should_panic(expected = "переполнение мантиссы i128")]
    fn multiplication_rejects_intermediate_integer_overflow() {
        let money = usd(Decimal::from_parts(u32::MAX, u32::MAX, u32::MAX, false, 2));

        let _ = money * u32::MAX;
    }

    #[test]
    fn wrong_scale_is_err_at_runtime_not_panic() {
        assert_eq!(Money::<Usd>::new(dec!(500_000.0)), Err(MoneyError::Scale));
        assert!(Money::<Usd>::new(dec!(500_000.00)).is_ok());
    }

    #[test]
    fn derived_ceiling_is_computed_at_build() {
        assert_eq!(MAX_DAILY_LOSS.amount(), dec!(1_000_000.00));
    }

    #[test]
    fn config_limit_is_compared_with_the_same_operator() {
        let from_config = Money::<Usd>::new(dec!(900_000.00)).unwrap();
        assert!(from_config < MAX_DAILY_LOSS);
        assert!(!(from_config < WARN_DAILY_LOSS));
    }

    #[test]
    fn ascending_checks_runtime_tiers_too() {
        let ok = [dec!(1.00), dec!(2.00), dec!(3.00)].map(|d| Money::<Usd>::new(d).unwrap());
        let bad = [dec!(1.00), dec!(3.00), dec!(2.00)].map(|d| Money::<Usd>::new(d).unwrap());
        assert!(ascending(&ok));
        assert!(!ascending(&bad));
    }
}
