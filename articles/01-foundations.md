# Type-driven development в Rust. Newtype, ADT, uninhabited types, phantom types, typestate. Часть 1/4

В 2010 году Yaron Minsky читал гостевую лекцию в Гарварде.
Он работал в Jane Street — фирме, которая торгует на бирже и пишет торговые системы на OCaml, и лекция была про то,
как применять ML-языки в индустрии, где цена ошибки измеряется в деньгах.
Позже её стали называть «Effective ML». Именно там впервые публично прозвучала фраза, которую потом подхватили далеко за пределами OCaml — в F#, Elm, Haskell:

Make illegal states unrepresentable.

Сделай недопустимые состояния невыразимыми. Вместо проверок в рантайме — типы, в которых такие состояния попросту не выражаются.
Ошибка, пойманная компилятором, не доходит до реальных торгов.
А с Rust эта история связана напрямую. Первый компилятор Rust Грейдон Хоар(создатель языка) написал на OCaml — язык рос из ML-окружения и перенял оттуда отношение к типам как к инструменту проектирования.

Где Rust на этой шкале?

Rust ближе к функциональным языкам — и по идеологии, и по встроенным конструкциям. ADT с исчерпывающим match, дженерики, трейты как развитие тайп-классов, ассоциированные типы.
Именно эти инструменты и позволяют делать недопустимые состояния невыразимыми. Но при этом Rust — это low-level язык, который проектировался без GC, и поэтому ему пришлось пожертвовать частью выразительной мощи системы типов ради безопасности работы с памятью.
В этой серии статей разберем, как применять типы в Rust для проектирования программ.
Не будем углубляться в теорию типов, а сосредоточимся на практических приёмах и шаблонах, которые можно использовать уже сейчас, на стабильной версии Rust и на Nightly.

## Newtype

Newtype — это `struct`-обёртка над существующим типом:

```rust
struct UserId(u64);
struct OrderId(u64);
```

Внутри каждой обёртки лежит обычный `u64`. В рантайме никаких накладных расходов: после компиляции newtype исчезает, остаётся `u64`. Но для компилятора `UserId` и `OrderId` — разные типы, и перепутать их в коде нельзя:

```rust
fn cancel_order(id: OrderId) { /* ... */ }

let user = UserId(42);
cancel_order(user);
// error[E0308]: expected `OrderId`, found `UserId`
```

Одно и то же представление в памяти, но компилятор различает значения по смыслу. Зачем это нужно — лучше всего видно на конкретной проблеме.

### Проблема

Если секрет ходит по коду как `String`, он утечёт через любую точку, где `String` обрабатывается «как обычная строка».

Скажем, мы храним пароли правильно — в виде криптографического хеша, который посчитан через `argon2` и лежит в БД в формате PHC (`"$argon2id$v=19$m=65536,t=3,p=4$..."`). Никакого plaintext-а в памяти и тем более на диске нет. Структура `User` могла бы выглядеть так:

```rust 
struct User {
    username: String,
    password_hash: String,
}
```

Хеш — не plaintext, но утекать он всё равно не должен: при утечке хешей атакующий получает материал для офлайн-перебора (rainbow tables, словари, GPU). И поскольку поле — обычный `String`, оно беспрепятственно проходит через все места, где `String` обрабатывается:

```rust
// {:?} распечатает структуру целиком — хеш уезжает в лог
tracing::debug!(?user, "authenticating");

// Любая функция, принимающая String, проглотит хеш без жалоб
fn send_telemetry(text: String) { /* ... */ }
send_telemetry(user.password_hash);
```

Это классический bug class из security-аудитов. Тип `String` сам по себе не несёт никакой информации о том, что значение чувствительное. Нужно вынести это в тип.

### Решение: newtype с маскированным Debug

Минимальная обёртка — отдельный тип с переопределённым `Debug`:

```rust
use std::fmt;

pub struct Password(String);

impl fmt::Debug for Password {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Password(\"********\")")
    }
}
```

Уже на этом шаге обе проблемы из предыдущего раздела исчезают:

```rust
tracing::debug!(?user, "authenticating");
// User { username: "alice", password: Password("********") }

send_telemetry(user.password);
// error[E0308]: expected `String`, found `Password`
```

Утечки через `{:?}` больше нет, в функции, принимающие `String`, хеш не передать. Но пока что это просто обёртка — у неё нет ни конструктора, ни проверок, и поле даже не маркировано приватным. Дальше превратим её в полноценный smart constructor с настоящим хешированием.

### Smart constructor: `Password::hash`

Делаем поле приватным (пусть код живёт в файле `password.rs` — поле видно только внутри модуля) и добавляем конструктор, который сам хеширует через `argon2`.

В `Cargo.toml`:

```toml
[dependencies]
argon2 = { version = "0.5", features = ["std"] }
```

Feature `std` обязателен: без него `argon2::password_hash::rand_core::OsRng` не виден — по умолчанию `rand_core` подключается без `getrandom`, и `OsRng` оказывается за выключенной фичей.

```rust
use std::fmt;
use argon2::{
    Algorithm, Argon2, Params, Version,
    password_hash::{
        PasswordHash, PasswordHasher, PasswordVerifier, SaltString,
        rand_core::OsRng,
    },
};

pub struct Password(String); // приватное поле — снаружи модуля не сконструировать

impl Password {
    // OWASP-рекомендуемые параметры (2023)
    // https://cheatsheetseries.owasp.org/cheatsheets/Password_Storage_Cheat_Sheet.html
    const ARGON2_MEMORY_KIB: u32 = 65536; // 64 MiB
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
}
```

Что здесь поддерживает инвариант:

- Поле приватно — снаружи модуля `Password(some_string)` не скомпилируется.
- `Password::hash` — пока единственный путь создания. После него внутри `Password` гарантированно валидная PHC-строка, посчитанная нашими параметрами.

Параметры — рекомендация OWASP 2023: 64 МиБ памяти, 3 итерации, 4 параллельных пути, 32-байтовый выход.
Соль генерируется через `OsRng` — криптографически стойкий генератор операционной системы.

Проверку пароля делаем через библиотечный `verify_password`:

```rust
impl Password {
    pub fn verify(&self, password: &str) -> bool {
        PasswordHash::new(&self.0)
            .map(|hash| {
                Argon2::default()
                    .verify_password(password.as_bytes(), &hash)
                    .is_ok()
            })
            .unwrap_or(false)
    }
}
```

`Argon2::verify_password` сравнивает в константное время — атаки по таймингу не работают.
Никаких `==` в нашем коде, никакого собственного криптокода — только библиотечный вызов.

### Загрузка из БД: `from_hash`

Хеш, который уже посчитан и лежит в базе, надо как-то загрузить обратно в `Password`. Через `Password::hash` нельзя — он хеширует входную строку заново. Поэтому добавляется второй конструктор, который тоже валидирует:

```rust
impl Password {
    pub fn from_hash(hash: String) -> Result<Self, argon2::password_hash::Error> {
        PasswordHash::new(&hash)?; // парсим PHC-формат — если не валиден, вернём Err
        Ok(Self(hash))
    }
}
```

Инвариант теперь поддержан и на загрузке: битый или подменённый хеш отсеивается прямо на границе, до того как окажется внутри `Password`. Если в БД лежит мусор, вызов вернёт `Err`, и мы узнаём об этом сразу, а не в момент `verify`.

Альтернативно можно отказаться от валидации и принять строку на доверии:

```rust
impl Password {
    pub const fn from_hash_unchecked(hash: String) -> Self {
        Self(hash)
    }
}
```

Версия без проверок — это компромисс. Плюс: не платим временем парсинга на каждой загрузке из БД (а это десятки тысяч записей при холодном старте — может быть заметно). Минус: ответственность за валидность строки переходит на вызывающий код. Если по ошибке передать туда не-PHC-строку, `verify` будет всегда возвращать `false` — паники нет, но и пользователь не войдёт. И, что хуже, эту ошибку не поймать тестом на конкретный тип.

Поэтому `from_hash_unchecked` стоит звать только из строго доверенных мест: код, который читает из своей же БД, где формат гарантирован миграцией. В обычной ситуации лучше валидирующий `from_hash` — гарантии сильнее, цена обычно несущественна.

Это типичная развилка при моделировании инварианта на типах: smart constructor отвечает за «честное создание с нуля», валидирующий `from_hash` — за «реконструкцию из недоверенного хранилища с проверкой», а `from_hash_unchecked` — за «реконструкцию из доверенного хранилища без проверки». Все три способа поддерживают инвариант, просто разными гарантиями.

### Хорошие практики

Что обычно реализуют у newtype:

**`From` / `Into` — для обёрток без инварианта.** Если внутри `u64` и валидировать нечего, `From` даёт удобный синтаксис:

```rust
struct UserId(u64);

impl From<u64> for UserId {
    fn from(id: u64) -> Self {
        UserId(id)
    }
}

let id: UserId = 42.into();
```

Для типов со smart-constructor (как `Password`) `From<String>` реализовывать **нельзя** — он обходит весь нарратив с `Password::hash`. Единственные пути создания остаются `Password::hash(...)` и `Password::from_hash(...)`.

**`AsRef<str>` — для read-only доступа к внутреннему значению.** Когда нужно сравнить, посмотреть или передать как `&str`, но не дать менять:

```rust
impl AsRef<str> for Password {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

let stored: &str = password.as_ref();
db.execute("UPDATE users SET hash = ?", &[stored])?;
```

Явный вызов, явный тип — никакой автоматической магии.

**`Deref` для типов с инвариантом реализовывать не надо.**

Если такая реализация появляется:

```rust
use std::ops::Deref;

impl Deref for Password {
    type Target = str;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
```

— автоматически включается **deref coercion**: `&Password` начинает превращаться в `&str` в любом контексте, где компилятор ожидает строку. И вся защита, которую построил Debug-маскировщик, перестаёт работать:

```rust
fn write_audit(action: &str) {
    std::fs::write("/var/log/audit.log", action).unwrap();
}

let password = Password::hash("secret123")?;
write_audit(&password);
// Компилируется. Хеш уезжает в /var/log/audit.log
```

`&password` сам сходит за `&str`, никакого предупреждения, никакой явной конверсии. То есть вся изоляция, которую дала приватность поля и `Debug`-маскировщик, сводится на нет одной строчкой `impl Deref`.

`AsRef` той же утечки не даёт: он требует явного `.as_ref()`, и в коде везде видно, где мы достаём строку из секретного типа. Поэтому правило простое: `Deref` оставляйте за прозрачными обёртками (`Box<T>`, `Rc<T>` — где обёртка концептуально и есть содержимое), а для newtype с инвариантом — только `AsRef` и собственные методы.

### В библиотеках

Что мы используем напрямую:

- [`argon2`](https://docs.rs/argon2/) — хеширование, верификация, парсинг PHC-формата. `Argon2::hash_password` и `Argon2::verify_password` — оба с правильной обработкой соли и параметров.

Что стоит знать рядом:

- [`secrecy`](https://docs.rs/secrecy/) — отдельный случай, для **raw-секретов**, которые нельзя хешировать: API-ключи, JWT-токены, mTLS-приватные ключи. Делает то же, что наш `Password` (маскирует `Debug` + smart constructor), плюс зануляет память при дропе через `zeroize`. После `drop(secret)` значение в куче перезаписывается нулями.

В нашем случае `secrecy` избыточен: внутри `Password` лежит хеш, plaintext в памяти живёт только пока выполняется `hash` или `verify`. Но для хранения raw API-ключа `SecretString` даст дополнительный слой защиты.

## ADT (алгебраические типы данных)

ADT в Rust — это `struct` (произведение типов: все поля одновременно) и `enum` (сумма типов: одно из). Newtype-обёртки, которыми мы занимались выше, формально тоже structs. Теперь к самой содержательной части — суммам.

### Проблема: «одно из» через примитивы

Допустим, нужно вернуть результат входа пользователя. На первый взгляд, хватит `bool`:

```rust
fn authenticate(username: &str, password: &str) -> bool { /* ... */ }
```

Но `bool` теряет всё, кроме факта успеха. Если не вошли — почему? Неверный пароль? Аккаунт заблокирован? Слишком много попыток за минуту? Просрочен срок действия? Само значение `false` ничего об этом не говорит.

Тогда начинают городить кортежи и `Option`-ы:

```rust
fn authenticate(...) -> (bool, Option<String>, Option<UserId>, Option<Instant>);
```

`bool` для исхода, `Option<String>` для причины отказа, `Option<UserId>` для успешного логина, `Option<Instant>` для «когда снова можно попробовать». Все эти `Option`-ы независимы — компилятор позволит вернуть `(true, Some("locked"), Some(UserId(42)), Some(now))`, что бессмыслица: «успех с указанной причиной отказа и заблокированный».

Похожая проблема — на входе функций. Допустим, открытие файла с двумя флагами доступа:

```rust
fn open_file(path: &str, read: bool, write: bool) -> File { /* ... */ }
```

Два независимых `bool`-а дают четыре сочетания `(read, write)`, из которых валидных только три:

- `(true, true)` — чтение и запись;
- `(true, false)` — только чтение;
- `(false, true)` — только запись;
- `(false, false)` — открыть файл «без доступа», бессмыслица.

Четвёртое состояние представимо в типе, но смысла не имеет. Каждый вызывающий код должен где-то его проверять — паниковать, возвращать ошибку или просто рассчитывать, что «никто так делать не будет». Корень один: тип `(bool, bool)` несёт состояния, которых в домене нет.

### Решение: `enum`

`enum` — это сумма типов. Значение всегда находится ровно в одном из объявленных вариантов:

```rust
use std::time::{Duration, Instant};

pub enum AuthOutcome {
    Success { user_id: UserId, session: SessionToken },
    InvalidCredentials,
    AccountLocked { until: Instant },
    RateLimited { retry_after: Duration },
    PasswordExpired { user_id: UserId },
}
```

Невозможные сочетания теперь просто не выражаются: `Success` без сессии, `AccountLocked` без срока, «успех с причиной отказа» — таких форм в типе нет.

И главное — `match` обязан покрыть все варианты:

```rust
match outcome {
    AuthOutcome::Success { user_id, session } => login(user_id, session),
    AuthOutcome::InvalidCredentials => audit("invalid creds"),
    AuthOutcome::AccountLocked { until } => respond_locked(until),
    AuthOutcome::RateLimited { retry_after } => respond_throttled(retry_after),
    AuthOutcome::PasswordExpired { user_id } => redirect_to_reset(user_id),
}
```

Если завтра в enum добавится новый вариант (например, `EmailNotVerified`), компилятор подсветит каждый `match`, где он не обработан: `error[E0004]: non-exhaustive patterns`. Это и есть «illegal states unrepresentable» в чистом виде: невозможное не выражается, возможное нельзя проигнорировать.

Тот же принцип закрывает и пример с открытием файла. Вместо двух bool-ов — enum из трёх валидных вариантов:

```rust
pub enum FileMode {
    Read,
    Write,
    ReadWrite,
}
```

Четвёртое сочетание `(false, false)` просто некуда поместить — в типе его нет.

### Формы вариантов

`AuthOutcome` выше показывает несколько форм в одном enum-е. Разберём отдельно.

**Без данных — просто метка:**

```rust
pub enum Role {
    Admin,
    User,
    Guest,
}
```

В рантайме такой enum — это маленькое целое число (компилятор сам выбирает представление). Идеально для перечислений: ролей, состояний, флагов. Аналог `InvalidCredentials` из `AuthOutcome` — отказ как факт, без подробностей.

**С tuple-полями — одно или несколько неименованных значений:**

```rust
pub enum Token {
    Bearer(String),
    ApiKey(String),
    Jwt(String),
}
```

Все три варианта несут `String`, но компилятор их различает: `Bearer("...")` и `ApiKey("...")` — разные значения. Удобно, когда поле одно и его смысл очевиден из имени варианта.

**С именованными полями — то же, что struct, но внутри варианта:**

```rust
pub enum AuthOutcome {
    Success { user_id: UserId, session: SessionToken },
    AccountLocked { until: Instant },
    // ...
}
```

Когда полей несколько — имена читаются понятнее, чем порядок в кортеже.

**Все формы в одном `enum`.** В `AuthOutcome` есть и вариант без данных (`InvalidCredentials`), и с одним именованным полем (`RateLimited { retry_after }`), и с несколькими (`Success { user_id, session }`). Это нормальная практика: каждый вариант сам выбирает, какая форма ему подходит.

### Вложенные ADT

`enum`-варианты могут содержать другие `enum`-ы. В audit-логе хочется записывать и сам факт события, и его исход:

```rust
use std::net::IpAddr;

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
```

Внутри `AuthEvent::Attempt` лежит другой enum — `AuthOutcome`. `match` тогда вкладывается:

```rust
fn audit(event: &AuthEvent) {
    match event {
        AuthEvent::Attempt { outcome, ip, .. } => match outcome {
            AuthOutcome::Success { user_id, .. } =>
                tracing::info!(?user_id, %ip, "login ok"),
            AuthOutcome::InvalidCredentials =>
                tracing::warn!(%ip, "invalid login"),
            AuthOutcome::AccountLocked { .. } =>
                tracing::warn!(%ip, "locked attempt"),
            AuthOutcome::RateLimited { .. } =>
                tracing::warn!(%ip, "throttled"),
            AuthOutcome::PasswordExpired { user_id } =>
                tracing::info!(?user_id, "expired password"),
        },
        AuthEvent::Logout { user_id, .. } =>
            tracing::info!(?user_id, "logout"),
        AuthEvent::PasswordChanged { user_id, .. } =>
            tracing::info!(?user_id, "password changed"),
    }
}
```

Исчерпывающая проверка работает на любой глубине. Если в `AuthOutcome` появится новый вариант, компилятор укажет каждое место, где двухуровневый `match` нужно дополнить — включая ветки внутри `AuthEvent::Attempt`.

Вложенные ADT — это и есть тот рычаг, который превращает «illegal states unrepresentable» из лозунга в работающий инструмент. Модель домена строится из нескольких слоёв `enum` и `struct`, и компилятор берёт на себя проверку, что все возможные комбинации обработаны, а невозможные не выражаются.

### Хорошие практики

Что обычно прицепляют к enum-у, чтобы он жил полноценной жизнью.

**Поведение через методы.** Enum в Rust — полноценный тип, к нему можно прикреплять методы через `impl`:

```rust
impl AuthOutcome {
    pub fn is_success(&self) -> bool {
        matches!(self, AuthOutcome::Success { .. })
    }

    pub fn user_id(&self) -> Option<UserId> {
        match self {
            AuthOutcome::Success { user_id, .. } => Some(*user_id),
            AuthOutcome::PasswordExpired { user_id } => Some(*user_id),
            _ => None,
        }
    }
}
```

Это превращает enum из «формы данных» в полноценную единицу домена: у `AuthOutcome` есть и состояния (варианты), и поведение (методы).

**Реализация трейтов.** То же со стандартными и собственными трейтами:

```rust
impl std::fmt::Display for AuthOutcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthOutcome::Success { .. } => write!(f, "ok"),
            AuthOutcome::InvalidCredentials => write!(f, "invalid credentials"),
            AuthOutcome::AccountLocked { .. } => write!(f, "account locked"),
            AuthOutcome::RateLimited { .. } => write!(f, "rate limited"),
            AuthOutcome::PasswordExpired { .. } => write!(f, "password expired"),
        }
    }
}
```

После этого `format!("{outcome}")` и `println!("{outcome}")` работают так, будто `AuthOutcome` — обычный тип с `Display`. Стандартные `derive` (`Debug`, `Clone`, `PartialEq`, `Eq`, `Hash`, иногда `Copy`) на enum-ах тоже работают — мы их уже использовали в определениях выше.

**`#[non_exhaustive]` для библиотечного API.** Если enum публикуется как часть библиотеки и в будущем хочется иметь возможность добавлять новые варианты без поломки downstream-кода — пометьте его атрибутом:

```rust
#[non_exhaustive]
pub enum AuthOutcome {
    Success { user_id: UserId, session: SessionToken },
    InvalidCredentials,
    AccountLocked { until: Instant },
    RateLimited { retry_after: Duration },
    PasswordExpired { user_id: UserId },
}
```

Что меняется:

- Внутри вашей крейт-у всё работает как обычно — exhaustive `match` без `_`-ветки.
- В крейтах-потребителях `match` без `_` уже не пройдёт. Компилятор требует явный fallback на случай новых вариантов.
- Когда вы выпускаете новую версию с новым вариантом, чужие проекты собираются без правок.

Компромисс: внешний код теряет exhaustiveness-проверку, но взамен получает совместимость по версиям. Для внутренних типов (использующихся только в вашем же проекте) `#[non_exhaustive]` не нужен — там как раз и хочется ловить новые варианты компилятором во всех `match`-ах.

**Wildcards `_` в `match` — с осторожностью.** Соблазн написать в `match` ветку `_ => default`, чтобы покрыть «всё остальное», понятен. Но именно это лишает вас главного преимущества exhaustive matching: при добавлении нового варианта компилятор не подскажет, где его обработать — `_` проглотит молча.

```rust
// Опасный паттерн:
match outcome {
    AuthOutcome::Success { .. } => grant_access(),
    _ => deny(),  // если завтра добавится PasswordExpired,
                  // мы тихо начнём отказывать в доступе тем, кому
                  // надо просто редиректнуть на reset пароля
}
```

`_` оправдан, когда логика для нескольких вариантов реально одинаковая и не зависит от их полей — как в `user_id()` выше, где для всех вариантов кроме `Success` и `PasswordExpired` мы возвращаем `None`. В таком случае добавление нового варианта тоже автоматически попадёт в эту ветку, и это правильное поведение.

Правило: явно перечисляйте варианты, если их обработка может различаться. `_` — только когда вы сознательно хотите «всё остальное» и уверены, что новые варианты должны попадать сюда же.

### В стандартной библиотеке

`Option<T>` и `Result<T, E>` — это и есть ADT, причём обобщённые:

```rust
enum Option<T> {
    Some(T),
    None,
}

enum Result<T, E> {
    Ok(T),
    Err(E),
}
```

`Option<T>` — минимальный sum-type: «значение есть» или «значения нет», без всяких null. `Result<T, E>` — стандартный способ возвращать исход операций вместо exception-ов; мы уже видели его в `Password::hash` и `Password::from_hash`. Обе типа — generic ADT: варианты несут данные параметризованного типа.

## Uninhabited types (типы без значений)

В заголовке статьи это «пустые типы» — типы, у которых ни одного возможного значения не существует. Создать такое значение нельзя ни одной комбинацией кода.

### Проблема: компилятор не знает, что эта ветка не сработает

Допустим, мы реализуем `FromStr` для newtype-обёртки `Username`, которая принимает любую строку:

```rust
use std::str::FromStr;

pub struct Username(String);

impl FromStr for Username {
    type Err = String; // приходится объявить какой-то тип ошибки

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Username(s.to_string()))
    }
}
```

`Err` нам по логике не нужен — `from_str` принимает любую строку и ошибиться физически не может. Но трейт требует объявить тип ошибки, и `String` тут просто заглушка.

Каждый вызов теперь обязан обработать ветку `Err`:

```rust
match "alice".parse::<Username>() {
    Ok(name) => greet(name),
    Err(_) => unreachable!("by construction"),
}
```

`unreachable!()` — это не доказательство, это панический коммент. Сегодня вы написали «ошибки не будет», завтра кто-то поменял `from_str` так, что ошибка появилась — компилятор не подскажет ни одного места, где `unreachable!` теперь стал ложью. В проде упадёт паника.

### Решение: тип ошибки без значений

Сделаем тип ошибки таким, что значений в нём не существует в принципе. Простейший способ — пустой `enum`:

```rust
pub enum Never {}
```

Ни одного варианта, ни одного конструктора. Значение `Never` невозможно произвести ни одной строкой кода. В стандартной библиотеке такой тип уже определён — `std::convert::Infallible`:

```rust
use std::convert::Infallible;
use std::str::FromStr;

pub struct Username(String);

impl FromStr for Username {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Username(s.to_string()))
    }
}
```

Теперь `Result<Username, Infallible>` несёт информацию на уровне типа: ветка `Err` физически не может произойти. Каллер разворачивает результат без unwrap-а и без unreachable:

```rust
let name: Username = match "alice".parse::<Username>() {
    Ok(name) => name,
    Err(never) => match never {}, // пустой match — веток ноль
};
```

Пустой `match` на `Infallible` компилируется чисто: покрывать нечего, потому что значений нет. Это и есть «illegal states unrepresentable» применительно к ошибкам — невозможный исход не выражается значением, и обрабатывать его нечего.

Если кто-то заменит `Infallible` на тип с реальными вариантами — все эти `match never {}` перестанут компилироваться, и компилятор укажет каждое место, где исход теперь возможен. Гарантия поддерживается типом, а не комментарием.

### Хорошие практики

**Используйте `std::convert::Infallible`, а не свой `enum Never {}`.** Стандартный тип узнаваем, уже интегрирован в `TryFrom`/`FromStr`/future-комбинаторы и не требует никаких определений. Свой `enum Never {}` — это лишний шум в коде ради идентичной семантики.

**`match never {}` вместо `unwrap()` или `unreachable!()`.** Все три варианта в рантайме ведут себя одинаково (потому что ветка `Err` недостижима), но семантика на уровне типа разная:

```rust
// Хуже — выглядит как runtime-проверка, читатель не знает,
// гарантировано ли отсутствие Err или просто «обычно его нет»:
let name = "alice".parse::<Username>().unwrap();

// Лучше — пустой match говорит «значений в этом типе нет»,
// и если кто-то заменит Infallible на реальный enum, код перестанет компилироваться:
let name: Username = match "alice".parse::<Username>() {
    Ok(name) => name,
    Err(never) => match never {},
};
```

`unwrap` тут панически безопасен по построению, но не выражает гарантию. Пустой `match` — выражает.

**`Infallible` оправдан только когда нужно подогнаться под Result-форму трейта.** `FromStr`, `TryFrom`, future-комбинаторы — там нужен `type Err`, и если конкретная реализация не может ошибиться, `Infallible` — единственный правильный выбор. Для свободной функции, которой не нужно сидеть в этом интерфейсе, просто верните `T`:

```rust
// Лишнее — Result<i32, Infallible> здесь это ceremony без пользы:
fn always_ok() -> Result<i32, Infallible> { Ok(42) }

// Правильно:
fn always_ok() -> i32 { 42 }
```

Если вы видите `Result<T, Infallible>` в свободной функции без явного интерфейс-требования — это запах кода.

### В стандартной библиотеке

`Infallible` живёт в `std::convert` и встречается там, где трейт-интерфейс требует тип ошибки, а конкретная реализация ошибиться не может:

- `impl<T> TryFrom<T> for T` с `type Error = Infallible` — конверсия типа в самого себя точно не падает.
- `impl FromStr for ...` для типов, принимающих любую строку без проверок (как наш `Username`).
- В future-комбинаторах и каналах, где определённые исходы исключены контрактом.

У `Infallible` есть синтаксический аналог через nightly-`!` (never type) — но он стабилизирован пока только как тип divergent-функций (`fn diverge() -> !` для `panic!`/`loop {}`/`return`). В произвольных позициях типа (`Result<T, !>`, `Vec<!>`) — пока за feature `never_type`, об этом — в [Части 4 (Nightly)](./04-nightly.md). До стабилизации используется `Infallible`.

В следующих разделах статьи uninhabited types работают неявно — они лежат под капотом phantom types и typestate, к которым переходим дальше.

## Phantom types

Phantom types — это параметры типа, которые присутствуют только для компилятора и не имеют рантайм-представления.

### Проблема: дублирующиеся newtype-ы

В разделе про newtype у нас получились `UserId(u64)` и `OrderId(u64)` — обёртки над одним и тем же `u64`, с одной и той же логикой. Если завтра появятся `ProductId`, `InvoiceId`, `TransactionId` — у каждого будет идентичный набор методов: `new`, `raw`, `Debug`, `Clone`, `From<u64>`. Пять одинаковых структур, пять одинаковых impl-блоков. Дублирование.

```rust
pub struct UserId(u64);
pub struct OrderId(u64);
pub struct ProductId(u64);
// ...копипаста для каждого нового идентификатора
```

### Решение: один generic-тип с маркером

Можно завести одну параметризованную структуру и набор пустых типов-маркеров, чтобы компилятор различал её «варианты»:

```rust
use std::marker::PhantomData;

pub struct Id<Tag> {
    raw: u64,
    _tag: PhantomData<Tag>,
}

pub struct User;
pub struct Order;

pub type UserId = Id<User>;
pub type OrderId = Id<Order>;
```

`PhantomData<Tag>` — это marker нулевого размера. Он говорит компилятору «этот тип параметризован по `Tag`», при этом значение `Tag` нигде не хранится в рантайме.

Зачем он вообще нужен: если просто написать `struct Id<Tag> { raw: u64 }` без `PhantomData`, компилятор скажет `error[E0392]: parameter \`Tag\` is never used`. `PhantomData<Tag>` — это техническая необходимость, чтобы `Tag` «считалось использованным», но без выделения памяти под него.

Что получаем:

- В памяти `Id<User>` и `Id<Order>` — один и тот же `u64`, 8 байт, никакого оверхеда.
- На этапе компиляции они — разные типы. `Id<User>` не передать туда, где ждут `Id<Order>`.
- Методы реализуются один раз — для `Id<Tag>` в общем виде:

```rust
impl<Tag> Id<Tag> {
    pub const fn new(raw: u64) -> Self {
        Self { raw, _tag: PhantomData }
    }

    pub const fn raw(&self) -> u64 {
        self.raw
    }
}
```

Использование:

```rust
fn cancel_order(id: OrderId) { /* ... */ }

let user_id = UserId::new(42);
cancel_order(user_id);
// error[E0308]: expected `Id<Order>`, found `Id<User>`
```

Та же гарантия, что и у newtype в первой секции, но с одной generic-реализацией для всех маркеров.

### Расширение: phantom-маркеры для секретов

Та же техника работает и для секретов. В реальном коде секреты бывают разных видов: JWT-ключ для подписи токенов, API-ключ для внешнего сервиса, ключ шифрования БД. Все они под капотом — `SecretString`, но смысл разный, и подменить один другим — серьёзный security-баг.

```rust
use secrecy::SecretString;

pub struct TaggedSecret<Kind> {
    inner: SecretString,
    _kind: PhantomData<Kind>,
}

pub struct JwtSigningKey;
pub struct ExternalApiKey;
pub struct DatabaseEncryptionKey;

fn sign_jwt(key: &TaggedSecret<JwtSigningKey>, payload: &str) -> String {
    // ...
}
```

Теперь функция подписи токенов принимает не любой секрет, а именно `TaggedSecret<JwtSigningKey>`. Передать туда `TaggedSecret<ExternalApiKey>` компилятор не даст — даже если оба внутри `SecretString` с одинаковым содержимым.

### Маркеры — какие типы брать

Маркеры (`User`, `Order`, `JwtSigningKey`) не несут данных и не должны существовать как значения — их роль чисто на уровне типа. Подходят два варианта:

**Unit struct** — `pub struct User;`. Можно сконструировать значение типа `User`, но это никому не нужно. Простой, привычный, дешёвый.

**Empty enum** — `pub enum User {}` (uninhabited type из прошлого раздела). Значение `User` создать нельзя в принципе. Гарантия, что маркер никогда не появится в рантайме случайно.

В большинстве случаев берут unit struct — короче и понятнее. Empty enum используется, когда хочется на уровне типа закрепить, что маркер существует только как метка и инстансов у него быть не может.

### Хорошие практики

**Поле для `PhantomData` называется `_tag` или `_marker`.** Подчёркивание показывает, что поле «техническое», и не используется напрямую. В сериализации его обычно пропускают (`#[serde(skip)]`).

**Конструируется как `PhantomData` без аргументов.** Это `const`-конструктор:

```rust
impl<Tag> Id<Tag> {
    pub const fn new(raw: u64) -> Self {
        Self { raw, _tag: PhantomData }
    }
}
```

**`PhantomData` не влияет на размер.** `size_of::<Id<User>>()` равен `size_of::<u64>()` — 8 байт. После компиляции от phantom не остаётся ничего.

**`#[derive(...)]` на `Id<Tag>` требует, чтобы `Tag` тоже его поддерживал.** Если написать `#[derive(Debug, Clone, PartialEq)] struct Id<Tag> { ... }`, компилятор сгенерирует `impl<Tag: Debug + Clone + PartialEq> Debug for Id<Tag>` — а маркеры по умолчанию ни одного из этих трейтов не реализуют, и тесты упадут с `error[E0277]: \`User\` doesn't implement \`Debug\``. Самый простой выход — навесить те же derive-ы и на маркеры:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct User;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Order;
```

Маркеры — это unit-struct-ы без данных, derive даёт тривиальные impl-ы (`Debug` печатает имя типа, `PartialEq` сравнивает «ничего с ничем» и всегда возвращает true), и этого достаточно, чтобы удовлетворить bound-ы на `Id<Tag>`.

Альтернатива — если маркер приходит из чужого кода и навесить на него derive нельзя — реализовать трейты для `Id<Tag>` вручную, без bound-ов на `Tag` (`impl<Tag> Debug for Id<Tag> { ... }`). Так сделано в std для `Rc<T>` и `PhantomData` сам по себе.

**Не злоупотребляйте, если хватает обычного newtype.** Для одного-двух типов newtype проще и читается прямолинейнее. Phantom выигрывает, когда:

- У вас семейство похожих структур (5+ tagged Id-ов);
- Generic-методы должны работать одинаково для всех маркеров;
- Маркеры могут добавляться извне (например, в библиотеке, которую расширяют пользователи).

### В стандартной библиотеке

`std::marker::PhantomData` — стандартный примитив, доступен и в `no_std`. Используется в самой std для двух целей:

- **Variance и владение** — `Rc<T>`, `Box<T>`, `Vec<T>` через `PhantomData<T>` говорят компилятору «я владею `T`», даже если физически хранят его не напрямую. Это влияет на проверки времени жизни и подтипирования.
- **Привязка lifetime-параметров** — `PhantomData<&'a T>` позволяет привязать lifetime к структуре, которая не хранит ссылку напрямую (например, итераторы по чужим данным).

Phantom types — это фундамент следующего раздела про **typestate**, где маркеры будут означать состояние объекта, а методы будут доступны только в нужных состояниях. К нему и переходим.

## Typestate

Кодируем состояние объекта в его типе. Методы, допустимые в одном состоянии и недопустимые в другом, перестают компилироваться вне нужного состояния. На вход компилятор получает не «объект с флагом», а «объект, у которого тип говорит, в каком он состоянии».

### Проблема: состояние через флаги

Многие процессы — авторизация, парсинг, TLS-handshake, БД-транзакция — это машины состояний с чётким порядком шагов. Если хранить состояние во флагах и `Option`-ах:

```rust
pub struct LoginAttempt {
    user_id: Option<UserId>,
    password_verified: bool,
    totp_verified: bool,
}

impl LoginAttempt {
    pub fn submit_credentials(&mut self, username: &str, password: &str) -> Result<(), AuthError> { /* ... */ }
    pub fn submit_totp(&mut self, code: &str) -> Result<(), AuthError> {
        if !self.password_verified {
            return Err(AuthError::OutOfOrder);
        }
        /* ... */
    }
    pub fn create_session(self) -> Result<Session, AuthError> {
        if !self.password_verified || !self.totp_verified {
            return Err(AuthError::IncompleteAuth);
        }
        /* ... */
    }
}
```

Те же беды, что в ADT-секции с двумя bool-ами:

- Можно вызвать `submit_totp` до `submit_credentials` — получим рантайм-ошибку, не compile-time;
- Можно вызвать `submit_credentials` повторно и затереть `user_id` — это тоже рантайм;
- Можно вызвать `create_session` на полпути — снова рантайм.

Компилятор все эти ошибки в дизайне пропускает. Они проявляются в тестах (если повезло) или в проде.

### Решение: каждое состояние — свой тип

Дадим каждому шагу процесса свой тип. Методы, допустимые в этом шаге, — в его `impl`-блоке. Переход на следующий шаг — это метод, который потребляет `self` и возвращает следующее состояние.

```rust
pub struct AwaitingCredentials;

pub struct AwaitingSecondFactor {
    user_id: UserId,
}

pub struct Authenticated {
    user_id: UserId,
}

impl AwaitingCredentials {
    pub fn new() -> Self {
        Self
    }

    pub fn submit_credentials(
        self,
        username: &str,
        password: &str,
    ) -> Result<AwaitingSecondFactor, AuthError> {
        let (user_id, stored_password) = lookup_user(username)
            .ok_or(AuthError::UserNotFound)?;
        if !stored_password.verify(password) {
            return Err(AuthError::InvalidCredentials);
        }
        Ok(AwaitingSecondFactor { user_id })
    }
}

impl AwaitingSecondFactor {
    pub fn submit_totp(self, code: &str) -> Result<Authenticated, AuthError> {
        verify_totp(self.user_id, code)?;
        Ok(Authenticated { user_id: self.user_id })
    }
}

impl Authenticated {
    pub fn create_session(self) -> Session {
        Session::new(self.user_id)
    }
}
```

Что это даёт:

```rust
let attempt = AwaitingCredentials::new();
attempt.create_session();
// error[E0599]: no method named `create_session` found for struct `AwaitingCredentials`
```

`create_session` физически невозможно вызвать в состоянии `AwaitingCredentials` — этого метода в его `impl`-блоке нет. То же с `submit_totp`: он есть только на `AwaitingSecondFactor`.

Нормальный поток выглядит цепочкой:

```rust
let attempt = AwaitingCredentials::new();
let in_progress = attempt.submit_credentials("alice", "...")?;
let authenticated = in_progress.submit_totp("123456")?;
let session = authenticated.create_session();
```

Каждый переход потребляет `self`. Старое состояние после `submit_credentials` физически больше не существует — `attempt` после перехода `moved`, и повторно его не использовать. Это и есть «illegal states unrepresentable» в самой строгой форме: невалидный порядок шагов не выражается в типе, и компилятор не пускает вызов метода вне его состояния.

### Phantom-вариант: один тип, разные состояния

Подход выше использует три отдельных структуры. Альтернатива — одна структура с phantom-параметром:

```rust
use std::marker::PhantomData;

pub struct LoginAttempt<State> {
    user_id: UserId,
    _state: PhantomData<State>,
}

#[derive(Debug, Clone, Copy)]
pub struct AwaitingSecondFactor;

#[derive(Debug, Clone, Copy)]
pub struct Authenticated;

impl LoginAttempt<AwaitingSecondFactor> {
    pub fn submit_totp(self, code: &str) -> Result<LoginAttempt<Authenticated>, AuthError> {
        verify_totp(self.user_id, code)?;
        Ok(LoginAttempt {
            user_id: self.user_id,
            _state: PhantomData,
        })
    }
}

impl LoginAttempt<Authenticated> {
    pub fn create_session(self) -> Session {
        Session::new(self.user_id)
    }
}
```

Phantom-вариант удобен, когда:

- Состояния делят значительную часть полей (как `user_id` здесь);
- Хочется один общий тип для логирования / сериализации / Debug;
- Нужны generic-методы, работающие на всех состояниях сразу.

Разные структуры удобнее, когда:

- У каждого состояния свой набор данных, разный по форме;
- Начальное состояние вообще ничего не содержит (как `AwaitingCredentials` выше — там нет ни `user_id`, ни чего бы то ни было).

В реальном коде часто смешивают: начальное состояние — отдельная структура, последующие — generic с phantom-параметром.

### Хорошие практики

**Потребляйте `self`, а не `&mut self` в переходах.** `&mut self` оставляет старое состояние доступным — можно вызвать переход дважды или забыть про новое значение. Потребление `self` гарантирует, что старое значение физически уничтожено и вызывающий обязан работать с новым:

```rust
// Плохо — &mut self оставляет attempt доступным после перехода:
fn submit_credentials(&mut self, ...) { /* ... */ }

// Хорошо — self передан по значению, attempt больше не существует:
fn submit_credentials(self, ...) -> Result<NextState, _> { /* ... */ }
```

**Маркеры состояний — публичные unit-struct-ы (или пустые enum-ы).** Caller-у нужно говорить «вот тебе `LoginAttempt<AwaitingSecondFactor>`», значит, маркер должен быть `pub`. Те же derive-ы, что и у parent-struct (см. ловушку из phantom-раздела).

**Имена состояний — фразы, описывающие точку процесса.** `AwaitingSecondFactor`, `Connected`, `RequestSigned` — лучше, чем `State2`, `Phase1`. По типу должно читаться, в каком шаге процесса мы находимся.

**Не пытайтесь выразить через typestate всё подряд.** Если состояний больше пяти-шести и переходы между ними произвольные — это лучше моделируется обычным ADT (`enum Status { ... }`). Typestate хорошо подходит, когда есть линейный или почти линейный порядок шагов; для произвольного графа переходов проще `enum + match`.

**Состояния-«тупики» через uninhabited-маркеры.** Если из состояния нет выхода (например, `Closed` после ошибки), маркер можно сделать пустым enum-ом — `pub enum Closed {}`. Это документирует на уровне типа, что состояние терминальное.

### Где встречается в реальности

- **TLS-handshake**: `rustls::client::ClientConnection` проходит фазы `Handshaking → Established`; некорректный порядок API-вызовов не компилируется.
- **Embedded HAL-крейты**: GPIO-пины с состояниями `Input`, `Output<PushPull>`, `Alternate<AF1>` — типичный typestate, который запрещает писать в пин, настроенный на чтение.
- **Криптографические протоколы**: библиотеки типа `noise-protocol` строят handshake-машины через типы фаз.
- **Builder-паттерны с обязательными полями**: крейты `typed-builder`, `bon`, `derive_builder` (в строгом режиме) превращают struct в typestate-builder, где компилятор не пускает `.build()` до тех пор, пока все обязательные поля не заполнены.
- **БД-транзакции**: в крейтах типа `sqlx` транзакция — это отдельный тип, у которого нельзя вызвать `commit` дважды.

### Связь с остальной серией

Typestate — это финал Части 1. Он собирает все предыдущие приёмы в один паттерн:

- **Newtype** — каждое состояние это отдельный тип со своими инвариантами и приватным полем.
- **ADT** — у объекта одно из заранее объявленных состояний, и переходы между ними явные.
- **Uninhabited types** — для терминальных состояний (из которых нельзя выйти) или для маркеров без значений используются пустые enum-ы.
- **Phantom types** — параметризация одной структуры разными состояниями без рантайм-оверхеда.

В **Части 2 (Контракты)** заберём этот фундамент в трейты, ассоциированные типы и const generics — то есть в инструменты, которыми задают _интерфейсы_, под которые пишут код _другие_. Typestate-машина становится частью контракта: «реализатор обязан проходить через эти состояния».