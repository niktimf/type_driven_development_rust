# Type-driven development в Rust. Newtype, ADT, uninhabited types, phantom types, typestate. Часть 1/4

Часто при проектировании сервиса кажется, что модель данных простая и закрытая: пользователь, заказ, токен сессии.
Всё выглядит логично — ровно до момента, когда окажется, что типы допускают кучу комбинаций полей, которых быть не должно.
Каждую такую комбинацию приходится отсекать руками — вложенными `if`-ами и `match`-ами по полям: «если ордер рыночный, но с ним пришла лимитная цена», «если ордер уже отменён, но всё ещё висит в стакане».
Такие проверки расползаются по всему коду.
Чем сложнее модель, тем больше таких дыр появляется, и, самое главное, есть риск пропустить какую-то из них, потому что компилятор не может помочь: он не знает, что эти состояния недопустимые.
В этой статье Никита Тимофеенко, разработчик команды MXDR, расскажет, как в Rust убрать часть таких комбинаций ещё на этапе описания типов.

В 2010 году Ярон Мински (Yaron Minsky) прочитал гостевую лекцию «Effective ML» студентам Гарварда, изучавшим OCaml.
Он работал в Jane Street — фирме, которая торгует на бирже и пишет торговые системы на OCaml, лекция была про то,
как применять ML-языки в индустрии, где цена ошибки измеряется в деньгах.
Именно там прозвучала фраза, которую потом стали цитировать далеко за пределами OCaml:

Make illegal states unrepresentable.

Сделай недопустимые состояния невыразимыми.
Вместо проверок в рантайме — типы, в которых такие состояния попросту не выражаются.
Ошибка, пойманная компилятором, не доходит до реальных торгов.
А с Rust эта история связана напрямую. Первый компилятор Rust Грейдон Хоар (Graydon Hoare, создатель языка) написал на OCaml. От ML-семьи Rust взял ADT с исчерпывающим `match`, паттерн-матчинг, type inference, замыкания, итераторы-комбинаторы, ассоциированные типы; трейты — в духе type classes из Haskell. Именно эти инструменты и позволяют делать недопустимые состояния невыразимыми.

При этом Rust — low-level язык без GC, и ему пришлось пожертвовать частью выразительной мощи системы типов ради безопасности работы с памятью. По выразительности он не дотягивает до Idris или Agda, но из мейнстримных языков — один из самых выразительных, и type-driven подходы на нём вполне работают.

В этой серии статей разберём, как применять типы в Rust для проектирования программ. Не будем углубляться в теорию типов, а сосредоточимся на практических приёмах и шаблонах, которые можно использовать уже сейчас, на стабильной версии Rust и на Nightly.

## Newtype

Newtype — это `struct`- обёртка над существующим типом:

```rust
struct UserId(u64);
struct OrderId(u64);
```

Внутри каждой обёртки лежит обычный `u64`.
В рантайме обёртка не добавляет накладных расходов — представление в памяти такое же, как у внутреннего `u64`. Если нужна гарантия одинакового layout (например, для FFI), указывают `#[repr(transparent)]`.
Но для компилятора `UserId` и `OrderId` — разные типы, и перепутать их в коде нельзя:

```rust
fn cancel_order(id: OrderId) { /* ... */ }

let user = UserId(42);
cancel_order(user);
// error[E0308]: expected `OrderId`, found `UserId`
```

Одно и то же представление в памяти, но компилятор различает значения по смыслу.
Зачем это нужно — лучше всего видно на конкретной проблеме.

### Проблема

`UserId` и `OrderId` спасают от перепутанных идентификаторов. Но у newtype есть и вторая роль — навесить на примитив смысл и инвариант. Виднее всего на числах, которые легко перепутать местами.

Выставляем лимитный ордер на бирже. В лоб сигнатура выглядит так:

```rust
fn place_limit_order(symbol: &str, is_buy: bool, price: f64, quantity: f64) -> OrderId { /* ... */ }
```

Здесь сразу несколько мин:

- `price` и `quantity` — оба `f64`, перепутать их местами компилятор не заметит:

```rust
// хотели: цена 185.50 за 10 штук
place_limit_order("AAPL", true, 10.0, 185.50);
// получили: цена 10.0 за 185.5 штук — и это спокойно скомпилировалось
```

- `f64` для денег теряет точность: `0.1 + 0.2` не равно `0.3`, и на тысячах ордеров копеечные расхождения складываются в реальные деньги.
- `price` может оказаться отрицательной, нулём или `NaN` — `f64` это допускает, а биржа отвергнет такой ордер уже в проде.
- `is_buy: bool` болеет тем же, что мы разберём в ADT-секции: `true` — это «купить» или «продать»? По типу не прочитать.

`f64`, `bool` и `&str` ничего не знают о домене. Вынесем смысл в типы.

### Решение: newtype под цену и объём

Заведём отдельные типы под цену и количество — обёртки над `Decimal` (почему `Decimal`, а не `f64`, — сразу после кода):

```rust
use rust_decimal::Decimal;

pub struct Price(Decimal);
pub struct Quantity(Decimal);
```

Теперь сигнатура читается сама, а аргументы не перепутать:

```rust
// Side — enum «купить/продать», к нему вернёмся в ADT-секции
fn place_limit_order(symbol: &str, side: Side, price: Price, quantity: Quantity) -> OrderId;

place_limit_order("AAPL", Side::Buy, quantity, price);
// error[E0308]: expected `Price`, found `Quantity`
```

`Price` и `Quantity` — разные типы, хотя внутри у обоих `Decimal`. Это та же защита, что `UserId` против `OrderId`, только теперь обёртка вдобавок несёт инвариант — положительность и шаг цены. Его и обеспечит smart constructor.

Почему `Decimal`, а не `f64`. Деньги нельзя считать в двоичной плавающей точке: `0.1 + 0.2` в `f64` даёт `0.30000000000000004`, и на потоке ордеров ошибка копится. `Decimal` хранит число десятичным — `0.1 + 0.2` ровно `0.3`, нет `NaN` и бесконечностей, а сравнения и остаток от деления точны. Последнее важно: проверку «цена кратна шагу» делают через `%`, и на `f64` она бы врала.

### Smart constructor: `InstrumentSpec`

У `Price` должен держаться инвариант: цена строго положительна и кратна шагу цены (tick size). Но шаг — не свойство самой цены: у одной бумаги тик `0.01`, у фьючерса — `0.25`, у крипто-пары — `0.0001`. Шаг принадлежит инструменту, поэтому и конструктор живёт на его спецификации.

Поле `Price` делаем приватным (код в модуле `market`), собрать в обход нельзя:

```rust
use rust_decimal::{Decimal, RoundingStrategy};

pub struct Price(Decimal);      // приватные поля
pub struct Quantity(Decimal);

// Шаги — тоже newtype, иначе в InstrumentSpec их можно перепутать местами
pub struct TickSize(Decimal);   // шаг цены
pub struct LotSize(Decimal);    // шаг объёма

#[derive(Debug)]
pub enum PriceError {
    NonPositive,
    NotOnTick { tick: Decimal },
}

impl TickSize {
    pub fn new(step: Decimal) -> Result<Self, PriceError> {
        if step <= Decimal::ZERO {
            return Err(PriceError::NonPositive);
        }
        Ok(Self(step))
    }

    pub fn amount(&self) -> Decimal {
        self.0
    }
}
// LotSize устроен так же

pub struct InstrumentSpec {
    pub tick_size: TickSize,
    pub lot_size: LotSize,
    // ...min/max, валюта котировки
}

impl InstrumentSpec {
    /// Строгий конструктор: некратная цена — ошибка, а не повод округлить молча.
    pub fn price(&self, value: Decimal) -> Result<Price, PriceError> {
        let tick = self.tick_size.amount();
        if value <= Decimal::ZERO {
            return Err(PriceError::NonPositive);
        }
        if value % tick != Decimal::ZERO {
            return Err(PriceError::NotOnTick { tick });
        }
        Ok(Price(value))
    }
}
```

Что здесь поддерживает инвариант:

- Поле приватно — снаружи модуля `Price(some_decimal)` не собрать.
- `InstrumentSpec::price` — единственный честный путь создания. После него внутри `Price` гарантированно положительное число на сетке тика.
- `tick_size` и `lot_size` — тоже newtype (`TickSize`/`LotSize`), а не голые `Decimal`. Иначе при сборке `InstrumentSpec` их можно перепутать местами, и компилятор бы смолчал. Принцип применяется рекурсивно: спецификация собирается из типизированных шагов.

Проверку `> 0` приходится писать руками: `NonZero` из std бывает только для целых и гарантирует лишь `≠ 0`, а `Decimal` знаковый — `-5` он бы пропустил. Готового типа под инвариант «положительное И на сетке тика» нет — за этим и нужен smart constructor.

Почему строгий, а не округляющий. Тихо подвинуть цену к ближайшему тику — значит изменить заявку за спиной у того, кто её выставляет: на лимитке это другая цена исполнения, живые деньги. Поэтому `price` возвращает `Err`, а округление — отдельный явный шаг, где стратегию выбирает вызывающий:

```rust
impl InstrumentSpec {
    pub fn round_price(&self, value: Decimal, strategy: RoundingStrategy) -> Result<Price, PriceError> {
        let tick = self.tick_size.amount();
        let ticks = (value / tick).round_dp_with_strategy(0, strategy);
        self.price(ticks * tick)
    }
}
```

`RoundingStrategy` заставляет ответить явно: вверх, вниз или к ближайшему. Для покупки безопаснее вниз (не переплатить), для продажи — вверх; молчаливого «к ближайшему» по умолчанию здесь нет.

`Quantity` устроен зеркально — тот же конструктор на `InstrumentSpec`, только проверка идёт против `lot_size`: объём положителен и кратен шагу лота.

### Хорошие практики

Что обычно реализуют у newtype:

**`From` / `Into` — для обёрток без инварианта.** Если внутри лежит примитив и проверять нечего (как `UserId(u64)`), `From` даёт удобный синтаксис:

```rust
struct UserId(u64);

impl From<u64> for UserId {
    fn from(id: u64) -> Self {
        UserId(id)
    }
}

let id: UserId = 42.into();
```

А для `Price` реализовывать `From<Decimal>` **нельзя**: `From` не умеет вернуть ошибку, то есть обошёл бы проверку положительности и шага. Единственные пути создания — `InstrumentSpec::price` и `from_raw`. Если уж нужен трейт-конвертер — это `TryFrom`, с тем же `Result`, что и у smart constructor.

**Доступ к значению — через явный геттер, а не `AsRef`/`Deref`.** `Decimal` дёшев и `Copy`, так что хватает метода, возвращающего копию:

```rust
impl Price {
    pub fn amount(&self) -> Decimal {
        self.0
    }
}

let notional = price.amount() * quantity.amount();
```

Явный вызов, явный тип — никакой автоматической магии.

**`Deref` для типов с инвариантом реализовывать не надо.** `Deref` включает **deref coercion**: значение начинает само подставляться вместо `&Target` где угодно, в обход ваших методов. Для `Price` это просто протекающая абстракция, а для чувствительных обёрток — уже дыра в безопасности.

Скажем, секрет с маскированным `Debug` — API-ключ, которому нельзя попадать в логи:

```rust
use std::fmt;
use std::ops::Deref;

pub struct ApiKey(String);

impl fmt::Debug for ApiKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ApiKey(\"***\")")   // маскируем
    }
}

impl Deref for ApiKey {                // ← вот эта строка всё и ломает
    type Target = str;
    fn deref(&self) -> &str {
        &self.0
    }
}

fn log(line: &str) { /* пишет в файл */ }

let key = ApiKey("sk-secret".to_string());
log(&key);
// Компилируется: &ApiKey превратился в &str, ключ утёк в лог в обход маскировки
```

Маскировка `Debug` бессильна: `&key` сам сошёл за `&str` — ни предупреждения, ни явной конверсии. Поэтому `Deref` оставляют прозрачным обёрткам (`Box<T>`, `Rc<T>` — где обёртка концептуально и есть содержимое), а для newtype с инвариантом или секретом — только явные геттеры и собственные методы.

### В библиотеках

Что используем напрямую:

- [`rust_decimal`](https://docs.rs/rust_decimal/) — тип `Decimal` для денег и цен: десятичная арифметика без потери точности, без `NaN`/`Inf`, точные сравнения и остаток от деления. `RoundingStrategy` задаёт стратегию округления, макрос `dec!` — удобные литералы (`dec!(185.50)`).

Что стоит знать рядом:

- [`secrecy`](https://docs.rs/secrecy/) — для секретов вроде `ApiKey` из примера с `Deref`. `SecretString` маскирует `Debug` и вдобавок зануляет память при дропе через `zeroize`: после `drop(secret)` значение в куче перезаписывается нулями — на голом `String` так не выйдет.

`Price`, `Quantity` и спецификация инструмента живут отдельными value objects. Вместе их собирает заявка:

```rust
pub struct Order {
    instrument: InstrumentId,
    side: Side,
    price: Price,
    quantity: Quantity,
}
```

`Order` — это product type (`struct`: все поля сразу), а `Side` (купить/продать) — sum type (`enum`: одно из). Оба — алгебраические типы данных, к ним и переходим.

## ADT (алгебраические типы данных)

ADT в Rust — это `struct` (произведение типов: все поля одновременно) и `enum` (сумма типов: одно из). Newtype-обёртки, которыми мы занимались выше, формально тоже struct'ы. Теперь к самой содержательной части — суммам.

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

В рантайме такой enum — это целое число (тег) минимального подходящего размера; явно фиксируется через `#[repr(u8)]` и т. п. Идеально для перечислений: ролей, состояний, флагов. Аналог `InvalidCredentials` из `AuthOutcome` — отказ как факт, без подробностей.

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

**Разные формы в одном `enum`.** В `AuthOutcome` есть и вариант без данных (`InvalidCredentials`), и с одним именованным полем (`RateLimited { retry_after }`), и с несколькими (`Success { user_id, session }`). Это нормальная практика: каждый вариант сам выбирает, какая форма ему подходит (tuple-форму сюда же можно добавить — все три сосуществуют без проблем).

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

- Внутри вашего крейта всё работает как обычно — exhaustive `match` без `_`-ветки.
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

`_` оправдан, когда логика для нескольких вариантов реально одинаковая и не зависит от их полей — как в `user_id()` выше, где для всех вариантов кроме `Success` и `PasswordExpired` мы возвращаем `None`. Но это работает, только пока вы готовы сформулировать «для всех будущих вариантов поведение по умолчанию — такое-то». Если завтра появится `PasswordReset { user_id }`, для которого надо бы вернуть `Some(user_id)`, `_ => None` это молча скроет.

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

`Option<T>` — минимальный sum-type: «значение есть» или «значения нет», без всяких null. `Result<T, E>` — стандартный способ возвращать исход операций вместо exception-ов. Оба типа — generic ADT: варианты несут данные параметризованного типа.

## Uninhabited types (типы без значений или пустые типы)

Пустые типы — типы, у которых ни одного возможного значения не существует. Создать такое значение нельзя ни одной комбинацией кода.

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

Теперь `Result<Username, Infallible>` несёт информацию на уровне типа: ветка `Err` физически не может произойти. Вызывающий код разворачивает результат без unwrap-а и без unreachable:

```rust
let name: Username = match "alice".parse::<Username>() {
    Ok(name) => name,
    Err(never) => match never {}, // пустой match — веток ноль
};
```

Пустой `match` на `Infallible` компилируется чисто. Exhaustiveness-проверка работает по принципу «все варианты должны быть покрыты»; у `Infallible` вариантов нет, соответственно и покрывать нечего. Это и есть «illegal states unrepresentable» применительно к ошибкам — невозможный исход не выражается значением.

Если кто-то заменит `Infallible` на тип с реальными вариантами — все эти `match never {}` перестанут компилироваться, и компилятор укажет каждое место, где исход теперь возможен. Гарантия поддерживается типом, а не комментарием.

### Хорошие практики

**Используйте `std::convert::Infallible`, а не свой `enum Never {}`.** Стандартный тип узнаваем, уже используется в `TryFrom`, `FromStr` и других местах стандартной библиотеки, и не требует никаких определений. Свой `enum Never {}` — это лишний шум в коде ради идентичной семантики.

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

`unwrap` тут не запаникует по построению, но не выражает гарантию на уровне типа. Пустой `match` — выражает.

**`Infallible` оправдан только когда нужно соответствовать `Result`-форме трейта.** `FromStr`, `TryFrom` — там обязателен `type Err`, и если конкретная реализация не может ошибиться, `Infallible` — единственный правильный выбор. Для свободной функции, которой не нужно сидеть в этом интерфейсе, просто верните `T`:

```rust
// Result<i32, Infallible> — здесь не нужен:
fn always_ok() -> Result<i32, Infallible> { Ok(42) }

// Правильно:
fn always_ok() -> i32 { 42 }
```

### В стандартной библиотеке

`Infallible` живёт в `std::convert` и встречается там, где трейт-интерфейс требует тип ошибки, а конкретная реализация ошибиться не может:

- Blanket-импл `impl<T, U> TryFrom<U> for T where U: Into<T>` с `type Error = Infallible` — любая `Into`-конверсия (и идентичная `T -> T` в том числе) не падает по определению.
- `impl FromStr for ...` для типов, принимающих любую строку без проверок (как наш `Username`).
- В местах, где контракт исключает определённые исходы (например, конверсия типа в самого себя или каналы с типом ошибки `Infallible`).

А в `no_std` тот же тип доступен как `core::convert::Infallible` — `std` просто реэкспортит его.

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

`PhantomData<Tag>` — это маркер нулевого размера. Он говорит компилятору «этот тип параметризован по `Tag`», при этом значение `Tag` нигде не хранится в рантайме.

Зачем он вообще нужен: если просто написать `struct Id<Tag> { raw: u64 }` без `PhantomData`, компилятор скажет `error[E0392]: parameter Tag is never used`. `PhantomData<Tag>` — это техническая необходимость, чтобы `Tag` «считалось использованным», но без выделения памяти под него.

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

**Empty enum** — `pub enum User {}` (uninhabited type из прошлого раздела). Значение `User` создать нельзя в принципе. Полезно, когда маркеры публичные и хочется на уровне типа запретить пользователю API случайно объявить `let _: User = ...;` или вернуть `User` из функции.

В большинстве случаев берут unit struct — короче и понятнее. Empty enum используется, когда хочется на уровне типа закрепить, что маркер существует только как метка и инстансов у него быть не может.

### Хорошие практики

**Поле для `PhantomData` называется `_tag` или `_marker`.** Подчёркивание — соглашение, говорящее «это техническое поле, к нему не обращаются напрямую». В сериализации его обычно пропускают (`#[serde(skip)]`).

**Конструируется как `PhantomData` без аргументов.** Это `const`-конструктор:

```rust
impl<Tag> Id<Tag> {
    pub const fn new(raw: u64) -> Self {
        Self { raw, _tag: PhantomData }
    }
}
```

**`PhantomData` не влияет на размер.** `size_of::<Id<User>>()` равен `size_of::<u64>()` — 8 байт. После компиляции от phantom не остаётся ничего.

**`#[derive(...)]` на `Id<Tag>` требует, чтобы `Tag` тоже его поддерживал.** Если написать `#[derive(Debug, Clone, PartialEq)] struct Id<Tag> { ... }`, компилятор сгенерирует `impl<Tag: Debug + Clone + PartialEq> Debug for Id<Tag>` — а маркеры по умолчанию ни одного из этих трейтов не реализуют, и тесты упадут с `error[E0277]: User doesn't implement Debug`. Самый простой выход — навесить те же derive-ы и на маркеры:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct User;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Order;
```

Маркеры — это unit-struct-ы без данных, derive даёт тривиальные impl-ы (`Debug` печатает имя типа, `PartialEq` сравнивает «ничего с ничем» и всегда возвращает true), и этого достаточно, чтобы удовлетворить bound-ы на `Id<Tag>`.

Альтернатива — если маркер приходит из чужого кода и навесить на него derive нельзя — реализовать трейты для `Id<Tag>` вручную, без bound-ов на `Tag` (`impl<Tag> Debug for Id<Tag> { ... }`). Это корректно: `Tag` физически не хранится, поэтому Debug-у нечего из него читать. Так в std сделано для самого `PhantomData<T>` — он реализует `Debug`, `Default`, `Clone`, `Copy` независимо от `T`.

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

Кодируем состояние объекта в его типе. Методы, допустимые в одном состоянии и недопустимые в другом, перестают компилироваться вне нужного состояния. Компилятор видит не «объект с флагом», а «объект, чей тип говорит, в каком он состоянии».

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

- **Embedded HAL-крейты**: GPIO-пины с состояниями `Input`, `Output<PushPull>`, `Alternate<AF1>` — типичный typestate, который запрещает писать в пин, настроенный на чтение.
- **Builder-паттерны с обязательными полями**: крейты `typed-builder` и `bon` превращают struct в typestate-builder, где компилятор не пускает `.build()` до тех пор, пока все обязательные поля не заполнены.
- **БД-транзакции**: в `sqlx::Transaction` методы `commit` и `rollback` принимают `self` по значению — повторный вызов невозможен, потому что транзакция перемещена.

### Связь с остальной серией

Typestate — финал Части 1: он собирает все предыдущие приёмы в один паттерн.

- **Newtype** — каждое состояние это отдельный тип со своими инвариантами и приватным полем.
- **ADT** — набор состояний и допустимых переходов фиксирован на этапе компиляции; только вместо вариантов одного `enum`-а — отдельные типы, а переходы — методы, потребляющие `self`.
- **Uninhabited types** — пустой `enum` в качестве маркера терминального состояния документирует на уровне типа, что выхода из него нет.
- **Phantom types** — параметризация одной структуры разными состояниями без рантайм-оверхеда.

В Части 2 («Контракты») этот фундамент уходит в трейты, ассоциированные типы и const generics — инструменты, которыми задают _интерфейсы_ для чужого кода.
Тогда typestate становится частью контракта трейта: «реализующий код обязан провести объект через эти состояния».