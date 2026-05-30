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
А с Rust эта история связана напрямую. Первый компилятор Rust Грейдон Хоар (Graydon Hoare, создатель языка) написал на OCaml.
От ML-семейства Rust взял ADT с исчерпывающим `match`, паттерн-матчинг, type inference, замыкания, итераторы-комбинаторы, ассоциированные типы; трейты — в духе type classes из Haskell.
Именно эти инструменты и позволяют делать недопустимые состояния невыразимыми.

При этом Rust — low-level язык без GC, и ему пришлось пожертвовать частью выразительной мощи системы типов ради безопасности работы с памятью.
По выразительности он не дотягивает до Idris, Agda или Haskell, но из мейнстримных языков — один из самых выразительных, и type-driven подходы на нём вполне работают.

В этой серии статей разберём, как применять типы в Rust для проектирования программ.
Не будем углубляться в теорию типов, а сосредоточимся на практических приёмах и шаблонах, которые можно использовать уже сейчас, на стабильной версии Rust и на Nightly.

## Newtype

Newtype — это `struct`-обёртка над существующим типом:

```rust
struct AccountId(u64);
struct OrderId(u64);
```

Внутри каждой обёртки лежит обычный `u64`.
В рантайме обёртка не добавляет накладных расходов — представление в памяти такое же, как у внутреннего `u64`. Если нужна гарантия одинакового layout (например, для FFI), указывают `#[repr(transparent)]`.
Но для компилятора `AccountId` и `OrderId` — разные типы, и перепутать их в коде нельзя:

```rust
fn cancel_order(id: OrderId) { /* ... */ }

let account = AccountId(42);
cancel_order(account);
// error[E0308]: expected `OrderId`, found `AccountId`
```

Одно и то же представление в памяти, но компилятор различает значения по смыслу.
Зачем это нужно — лучше всего видно на конкретной проблеме.

### Проблема

`AccountId` и `OrderId` спасают от перепутанных идентификаторов.
Но у newtype есть и вторая роль — навесить на примитив смысл и инвариант.
Виднее всего на числах, которые легко перепутать местами.

Выставляем лимитный ордер на бирже. В лоб сигнатура выглядит так:

```rust
fn place_limit_order(symbol: &str, is_buy: bool, price: f64, quantity: f64) -> OrderId { /* ... */ }
```

Здесь сразу несколько проблем:

- `price` и `quantity` — оба `f64`, перепутать их местами компилятор не заметит:

```rust
// хотели: цена 185.50 за 10 штук
place_limit_order("AAPL", true, 10.0, 185.50);
// получили: цена 10.0 за 185.5 штук — и это спокойно скомпилировалось
```

- `f64` для денег теряет точность: число с плавающей точкой не хранит десятичные дроби ровно, и на потоке ордеров копеечные расхождения складываются в реальные деньги.
- `price` может оказаться отрицательной, нулём или `NaN` — `f64` это допускает, а биржа отвергнет такой ордер уже в проде.

`f64` ничего не знает о домене — вынесем смысл в типы.
Флаг `is_buy: bool` пока оставим как есть: чем плох булев флаг и на что его заменить — разберём в ADT-секции.

### Решение: newtype под цену и объём

Заведём отдельные типы под цену и количество — обёртки над `Decimal`:

```rust
use rust_decimal::Decimal;

pub struct Price(Decimal);
pub struct Quantity(Decimal);
```

Теперь сигнатура читается сама, а аргументы не перепутать:

```rust
fn place_limit_order(symbol: &str, is_buy: bool, price: Price, quantity: Quantity) -> OrderId { /* ... */ }

place_limit_order("AAPL", true, quantity, price);
// error[E0308]: expected `Price`, found `Quantity`
```

`Price` и `Quantity` — разные типы, хотя внутри у обоих `Decimal`.
Это та же защита, что `AccountId` и `OrderId`, только теперь обёртка вдобавок несёт инвариант — положительность и шаг цены.
Его и обеспечит smart constructor.

Почему `Decimal`, а не `f64`. Деньги нельзя считать в числах с плавающей точкой: `0.1 + 0.2` в `f64` даёт `0.30000000000000004`, и на потоке ордеров ошибка копится.
`Decimal` хранит число десятичным — `0.1 + 0.2` ровно `0.3`, нет `NaN` и бесконечностей, а сравнения и остаток от деления точны.
Последнее важно: проверку «цена кратна шагу» делают через `%`, и на `f64` она бы врала.

### Smart constructor: `InstrumentSpec`

У `Price` должен держаться инвариант: цена строго положительна и кратна шагу цены (tick size).
Но шаг — не свойство самой цены: у одной бумаги тик `0.01`, у фьючерса — `0.25`, у крипто-пары — `0.0001`.
Шаг принадлежит инструменту, поэтому и конструктор живёт на его спецификации.

Это случай чуть хитрее классического.
Обычно smart constructor живёт на самом типе — `Price::new(value) -> Result<Price, _>`,
когда инвариант самодостаточен (скажем, «непустая строка» или «число в диапазоне `0..=100`): всё нужное для проверки уже в самом значении.
Здесь же инвариант зависит от внешнего контекста — шага инструмента, поэтому конструктор переезжает туда, где этот контекст есть.
Приём тот же — меняется лишь то, на каком типе живёт конструктор.

Поле `Price` делаем приватным (код в модуле `market`), собрать в обход нельзя:

```rust
use rust_decimal::Decimal;

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
- `tick_size` и `lot_size` — тоже newtype (`TickSize`/`LotSize`), а не голые `Decimal`.
Иначе при сборке `InstrumentSpec` их можно перепутать местами, и компилятор бы смолчал. Принцип применяется рекурсивно: спецификация собирается из типизированных шагов.

Почему конструктор строгий, а не округляющий. Тихо подвинуть цену к ближайшему тику — значит изменить заявку за спиной у того, кто её выставляет: на лимитке это другая цена исполнения, живые деньги.
Поэтому `price` возвращает `Err` на некратной цене. Если округление нужно — это отдельный явный шаг (`round_price`), где вызывающий сам выбирает стратегию: вниз для покупки, чтобы не переплатить, вверх для продажи.
Молчаливого «округлим к ближайшему» по умолчанию здесь нет — сам факт округления должен быть виден в коде.

`Quantity` устроен зеркально — тот же конструктор на `InstrumentSpec`, только проверка идёт против `lot_size`: объём положителен и кратен шагу лота.

### Хорошие практики

Что обычно реализуют у newtype:

**`From` / `Into` — для обёрток без инварианта.**
Если внутри лежит примитив и проверять нечего (как `AccountId(u64)`), `From` даёт удобный синтаксис:

```rust
struct AccountId(u64);

impl From<u64> for AccountId {
    fn from(id: u64) -> Self {
        AccountId(id)
    }
}

let id: AccountId = 42.into();
```

А для `Price` реализовывать `From<Decimal>` **нельзя**: `From` не умеет вернуть ошибку — он обошёл бы проверку положительности и кратности шагу. Не спасёт и `TryFrom<Decimal>`: ошибку он вернуть умеет, но для проверки нужен шаг инструмента, а взять его внутри `From`/`TryFrom` неоткуда. Поэтому единственный путь создания — `InstrumentSpec::price`. (Если бы инвариант был самодостаточным — скажем, «просто положительное число», — `TryFrom` подошёл бы, с тем же `Result`, что и у smart constructor.)

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

**`Deref` для типов с инвариантом реализовывать не надо.**
`Deref` включает **deref coercion**: значение начинает само подставляться вместо `&Target` где угодно, в обход ваших методов.
Для `Price` это просто протекающая абстракция, а для чувствительных обёрток — уже дыра в безопасности.

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

impl Deref for ApiKey {                // <- вот эта строка всё и ломает
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

Маскировка `Debug` бессильна: `&key` сам сошёл за `&str` — ни предупреждения, ни явной конверсии.
Поэтому `Deref` оставляют прозрачным обёрткам (`Box<T>`, `Rc<T>` — где обёртка концептуально и есть содержимое), а для newtype с инвариантом или секретом — только явные геттеры и собственные методы.

### В библиотеках

Что используем напрямую:

- [`rust_decimal`](https://docs.rs/rust_decimal/) — тип `Decimal` для денег и цен: десятичная арифметика без потери точности, без `NaN`/`Inf`, точные сравнения и остаток от деления. `RoundingStrategy` задаёт стратегию округления, макрос `dec!` — удобные литералы (`dec!(185.50)`).

Что стоит знать рядом:

- [`secrecy`](https://docs.rs/secrecy/) — для секретов вроде `ApiKey` из примера с `Deref`.
`SecretString` маскирует `Debug` и вдобавок зануляет память при дропе через `zeroize`: после `drop(secret)` значение в куче перезаписывается нулями — на голом `String` так не выйдет.

`Price`, `Quantity` и спецификация инструмента живут отдельными value objects. Вместе их собирает заявка:

```rust
pub struct Order {
    instrument: InstrumentId,
    side: Side,
    price: Price,
    quantity: Quantity,
}
```

`Order` — это product type (`struct`: все поля сразу), а `Side` (купить/продать) — sum type (`enum`: одно из).
Оба — алгебраические типы данных, к ним и переходим.

## ADT (алгебраические типы данных)

ADT в Rust — это `struct` (произведение типов: все поля одновременно) и `enum` (сумма типов: одно из).
Newtype-обёртки, которыми мы занимались выше, формально тоже struct'ы. Теперь к самой содержательной части — суммам.

### Проблема: «одно из» через примитивы

Вернёмся к заявке. У неё есть тип: рыночная (исполнить по любой доступной цене), лимитная (не хуже указанной цены) или стоп-лимитная (когда рынок дойдёт до триггера — выставить лимитку).
В лоб это кодируют флагом и парой `Option`-ов:

```rust
struct Order {
    side: Side,
    is_market: bool,
    limit_price: Option<Price>,
    stop_price: Option<Price>,
}
```

Поля независимы, и тип спокойно допускает бессмыслицу:

- `is_market: true` вместе с `limit_price: Some(...)` — ровно «рыночная заявка с лимитной ценой» из начала статьи;
- `is_market: false` и `limit_price: None` — лимитная заявка без цены;
- `stop_price: Some(...)` без лимитной цены — наполовину собранная стоп-лимитка.

Из восьми сочетаний валидны три, остальные — мусор, который каждый кусок кода обязан отсекать руками.
Та же болезнь, что у двух `bool`-флагов открытия файла `(read, write)` с бессмысленным `(false, false)` или у `submit(order) -> bool`, где `false` не говорит, почему заявку отклонили.
Корень один: набор независимых примитивов несёт состояния, которых в домене нет.

### Решение: `enum`

`enum` — это сумма типов. Значение всегда находится ровно в одном из объявленных вариантов, и каждый вариант несёт ровно те данные, которые ему нужны:

```rust
pub enum OrderType {
    Market,
    Limit(Price),
    StopLimit { stop: Price, limit: Price },
}
```

Невозможные сочетания теперь просто не выражаются: у `Market` нет поля цены — «рыночная заявка с лимитной ценой» не собирается;
лимитная без цены тоже не построится — `Limit` без `Price` не существует.
Этот `OrderType` и встаёт полем в `Order` вместо `is_market` и двух `Option`-ов: цена живёт там, где она осмысленна, и только там.

И главное — `match` обязан покрыть все варианты:

```rust
match order_type {
    OrderType::Market => execute_at_market(),
    OrderType::Limit(price) => place_in_book(price),
    OrderType::StopLimit { stop, limit } => arm_stop(stop, limit),
}
```

Если завтра добавится новый вариант (например, `TrailingStop`), компилятор подсветит каждый `match`, где он не обработан: `error[E0004]: non-exhaustive patterns`.
Это и есть «illegal states unrepresentable» в чистом виде: невозможное не выражается, возможное нельзя проигнорировать.

Тот самый `is_buy: bool` из `place_limit_order`, который мы отложили в newtype-разделе, лечится так же.
`true` — это «купить» или «продать»? По типу не прочитать, по значению легко перепутать. `enum` вместо флага:

```rust
pub enum Side {
    Buy,
    Sell,
}
```

Теперь в сигнатуре `place_limit_order(..., side: Side, ...)` сторона читается по имени варианта (`Side::Buy`), а не угадывается по `true`/`false`.

### Формы вариантов

`OrderType` выше показывает сразу три формы вариантов — разберём отдельно.

**Без данных — просто метка:**

```rust
pub enum Side {
    Buy,
    Sell,
}
```

В рантайме такой enum — это целое число (тег) минимального подходящего размера; явно фиксируется через `#[repr(u8)]` и т. п.
Идеально для перечислений: сторон, статусов, флагов. Аналог `OrderType::Market` — вариант как факт, без данных.

**С tuple-полями — одно или несколько неименованных значений:**

```rust
pub enum OrderType {
    Market,
    Limit(Price),   // <- tuple-вариант: одно поле
    // ...
}
```

`Limit(Price)` несёт цену без имени поля — этого хватает, потому что у лимитной заявки цена ровно одна и имя варианта уже всё объясняет.

**С именованными полями — то же, что struct, но внутри варианта:**

```rust
pub enum OrderType {
    // ...
    StopLimit { stop: Price, limit: Price },
}
```

Когда полей несколько и оба — `Price`, имена `stop`/`limit` спасают: по позиции `(Price, Price)` их легко перепутать, по именам — нет.

**Разные формы в одном `enum`.** `OrderType` соединяет все три: `Market` без данных, `Limit(Price)` с tuple-полем, `StopLimit { stop, limit }` с именованными.
Это нормальная практика: каждый вариант сам выбирает форму под себя.

### Вложенные ADT

`enum`-варианты могут содержать другие `enum`-ы. В журнале событий по заявке хочется фиксировать и сам факт, и детали:

```rust
pub enum CancelReason {
    ByUser,
    Expired,
}

pub enum OrderEvent {
    Accepted { order_type: OrderType, side: Side },
    Filled { price: Price, quantity: Quantity },
    Cancelled { reason: CancelReason },
}
```

Внутри `OrderEvent::Accepted` лежит другой enum — `OrderType`. `match` тогда вкладывается:

```rust
fn log_event(event: &OrderEvent) {
    match event {
        OrderEvent::Accepted { order_type, side } => match order_type {
            OrderType::Market =>
                tracing::info!(?side, "accepted market order"),
            OrderType::Limit(price) =>
                tracing::info!(?side, ?price, "accepted limit order"),
            OrderType::StopLimit { stop, limit } =>
                tracing::info!(?side, ?stop, ?limit, "accepted stop-limit"),
        },
        OrderEvent::Filled { price, quantity } =>
            tracing::info!(?price, ?quantity, "filled"),
        OrderEvent::Cancelled { reason } =>
            tracing::info!(?reason, "cancelled"),
    }
}
```

Исчерпывающая проверка работает на любой глубине. Если в `OrderType` появится новый вариант, компилятор укажет каждое место, где двухуровневый `match` нужно дополнить — включая ветки внутри `OrderEvent::Accepted`.

Вложенные ADT — это и есть тот рычаг, который превращает «illegal states unrepresentable» из лозунга в работающий инструмент.
Модель домена строится из нескольких слоёв `enum` и `struct`, и компилятор берёт на себя проверку, что все возможные комбинации обработаны, а невозможные не выражаются.

### Хорошие практики

Что обычно прицепляют к enum-у, чтобы он жил полноценной жизнью.

**Поведение через методы.** Enum в Rust — полноценный тип, к нему можно прикреплять методы через `impl`:

```rust
impl OrderType {
    pub fn is_market(&self) -> bool {
        matches!(self, OrderType::Market)
    }

    pub fn limit_price(&self) -> Option<Price> {
        match self {
            OrderType::Limit(price) => Some(*price),
            OrderType::StopLimit { limit, .. } => Some(*limit),
            OrderType::Market => None,
        }
    }
}
```

Это превращает enum из «формы данных» в полноценную единицу домена: у `OrderType` есть и состояния (варианты), и поведение (методы).

**Реализация трейтов.** То же со стандартными и собственными трейтами:

```rust
impl std::fmt::Display for OrderType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OrderType::Market => write!(f, "market"),
            OrderType::Limit(_) => write!(f, "limit"),
            OrderType::StopLimit { .. } => write!(f, "stop-limit"),
        }
    }
}
```

После этого `format!("{order_type}")` и `println!("{order_type}")` работают так, будто `OrderType` — обычный тип с `Display`.
Стандартные `derive` (`Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`, `Hash`) на enum-ах тоже работают — мы их уже использовали в определениях выше.

**`#[non_exhaustive]` для библиотечного API.** Если enum публикуется как часть библиотеки и в будущем хочется иметь возможность добавлять новые варианты без поломки downstream-кода — пометьте его атрибутом:

```rust
#[non_exhaustive]
pub enum OrderType {
    Market,
    Limit(Price),
    StopLimit { stop: Price, limit: Price },
}
```

Что меняется:

- Внутри вашего крейта всё работает, как обычно — exhaustive `match` без `_`-ветки.
- В крейтах-потребителях `match` без `_` уже не пройдёт. Компилятор требует явный fallback на случай новых вариантов.
- Когда вы выпускаете новую версию с новым вариантом (`TrailingStop`), чужие проекты собираются без правок.

Компромисс: внешний код теряет exhaustiveness-проверку, но взамен получает совместимость по версиям. Для внутренних типов (использующихся только в вашем же проекте) `#[non_exhaustive]` не нужен — там как раз и хочется ловить новые варианты компилятором во всех `match`-ах.

**Wildcards `_` в `match` — с осторожностью.** Соблазн написать в `match` ветку `_ => default`, чтобы покрыть «всё остальное», понятен. Но именно это лишает вас главного преимущества exhaustive matching: при добавлении нового варианта компилятор не подскажет, где его обработать — `_` проглотит молча.

```rust
// Опасный паттерн:
match order_type {
    OrderType::Market => route_to_market_engine(),
    _ => route_to_limit_book(),  // добавится TrailingStop —
                                 // и его молча отправят в стакан лимиток,
                                 // хотя ему нужна отдельная логика
}
```

`_` оправдан, когда логика для нескольких вариантов реально одинаковая и не зависит от их полей. Но это работает, только пока вы готовы сформулировать «для всех будущих вариантов поведение по умолчанию — такое-то». В `limit_price()` выше мы наоборот выписали `Market => None` явно: если завтра добавится тип заявки с ценой, компилятор заставит про него вспомнить, а не вернёт по-тихому `None`.

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

Допустим, мы реализуем `FromStr` для `ClientOrderId` — клиентского идентификатора заявки (в FIX это clOrdID): произвольной строки, которую задаёт сам клиент. Любая строка годится:

```rust
use std::str::FromStr;

pub struct ClientOrderId(String);

impl FromStr for ClientOrderId {
    type Err = String; // приходится объявить какой-то тип ошибки

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(ClientOrderId(s.to_string()))
    }
}
```

`Err` нам по логике не нужен — `from_str` принимает любую строку и ошибиться физически не может. Но трейт требует объявить тип ошибки, и `String` тут просто заглушка.

Каждый вызов теперь обязан обработать ветку `Err`:

```rust
match "order-2026-0001".parse::<ClientOrderId>() {
    Ok(id) => register(id),
    Err(_) => unreachable!("by construction"),
}
```

`unreachable!()` — это не доказательство, это панический коммент.
Сегодня вы написали «ошибки не будет», завтра кто-то поменял `from_str` так, что ошибка появилась — компилятор не подскажет ни одного места, где `unreachable!` теперь стал ложью. В проде будет паника.

### Решение: тип ошибки без значений

Сделаем тип ошибки таким, что значений в нём не существует в принципе. Простейший способ — пустой `enum`:

```rust
pub enum Never {}
```

Ни одного варианта, ни одного конструктора. Значение `Never` невозможно произвести ни одной строкой кода. В стандартной библиотеке такой тип уже определён — `std::convert::Infallible`:

```rust
use std::convert::Infallible;
use std::str::FromStr;

pub struct ClientOrderId(String);

impl FromStr for ClientOrderId {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(ClientOrderId(s.to_string()))
    }
}
```

Теперь `Result<ClientOrderId, Infallible>` несёт информацию на уровне типа: ветка `Err` физически не может произойти.
Вызывающий код разворачивает результат без unwrap-а и без unreachable:

```rust
let id: ClientOrderId = match "order-2026-0001".parse::<ClientOrderId>() {
    Ok(id) => id,
    Err(never) => match never {}, // пустой match — веток ноль
};
```

Пустой `match` на `Infallible` компилируется чисто.
Exhaustiveness-проверка работает по принципу «все варианты должны быть покрыты»; у `Infallible` вариантов нет, соответственно и покрывать нечего.
Это и есть «illegal states unrepresentable» применительно к ошибкам — невозможный исход не выражается значением.

Если кто-то заменит `Infallible` на тип с реальными вариантами — все эти `match never {}` перестанут компилироваться, и компилятор укажет каждое место, где исход теперь возможен.
Гарантия поддерживается типом, а не комментарием.

### Хорошие практики

**Используйте `std::convert::Infallible`, а не свой `enum Never {}`.**
Стандартный тип узнаваем, уже используется в `TryFrom`, `FromStr` и других местах стандартной библиотеки, и не требует никаких определений.
Свой `enum Never {}` — это лишний шум в коде ради идентичной семантики.

**`match never {}` вместо `unwrap()` или `unreachable!()`.**
Все три варианта в рантайме ведут себя одинаково (потому что ветка `Err` недостижима), но семантика на уровне типа разная:

```rust
// Хуже — выглядит как runtime-проверка, читатель не знает,
// гарантировано ли отсутствие Err или просто «обычно его нет»:
let id = "order-2026-0001".parse::<ClientOrderId>().unwrap();

// Лучше — пустой match говорит «значений в этом типе нет»,
// и если кто-то заменит Infallible на реальный enum, код перестанет компилироваться:
let id: ClientOrderId = match "order-2026-0001".parse::<ClientOrderId>() {
    Ok(id) => id,
    Err(never) => match never {},
};
```

`unwrap()` тут не запаникует по построению, но не выражает гарантию на уровне типа. Пустой `match` — выражает.

**`Infallible` оправдан только когда нужно соответствовать `Result`-форме трейта.** `FromStr`, `TryFrom` — там обязателен `type Err`, и если конкретная реализация не может ошибиться, `Infallible` — единственный правильный выбор.
Для свободной функции, которой не нужно сидеть в этом интерфейсе, просто верните `T`:

```rust
// Result<i32, Infallible> — здесь не нужен:
fn always_ok() -> Result<i32, Infallible> { Ok(24) }

// Правильно:
fn always_ok() -> i32 { 24 }
```

### В стандартной библиотеке

`Infallible` живёт в `std::convert` и встречается там, где трейт-интерфейс требует тип ошибки, а конкретная реализация ошибиться не может:

- Blanket-импл `impl<T, U> TryFrom<U> for T where U: Into<T>` с `type Error = Infallible` — любая `Into`-конверсия (и идентичная `T -> T` в том числе) не падает по определению.
- `impl FromStr for ...` для типов, принимающих любую строку без проверок (как наш `ClientOrderId`).
- В местах, где контракт исключает определённые исходы (например, конверсия типа в самого себя или каналы с типом ошибки `Infallible`).

А в `no_std` тот же тип доступен как `core::convert::Infallible` — `std` просто реэкспортит его.

У `Infallible` есть синтаксический аналог через nightly-`!` (never type) — но он стабилизирован пока только как тип divergent-функций (`fn diverge() -> !` для `panic!`/`loop {}`/`return`). В произвольных позициях типа (`Result<T, !>`, `Vec<!>`) — пока за feature `never_type`, об этом — в [Части 4 (Nightly)](./04-nightly.md). До стабилизации используется `Infallible`.

В следующих разделах статьи uninhabited types работают неявно — они лежат под капотом phantom types и typestate, к которым переходим дальше.

## Phantom types

Phantom types — это параметры типа, которые присутствуют только для компилятора и не имеют рантайм-представления.

### Проблема: дублирующиеся newtype-ы

В разделе про newtype у нас получились `AccountId(u64)` и `OrderId(u64)` — обёртки над одним и тем же `u64`, с одной и той же логикой. Если завтра появятся `InstrumentId`, `TradeId`, `PositionId` — у каждого будет идентичный набор методов: `new`, `raw`, `Debug`, `Clone`, `From<u64>`. Пять одинаковых структур, пять одинаковых impl-блоков. Дублирование.

```rust
pub struct AccountId(u64);
pub struct OrderId(u64);
pub struct InstrumentId(u64);
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

pub struct Order;
pub struct Instrument;

pub type OrderId = Id<Order>;
pub type InstrumentId = Id<Instrument>;
```

`PhantomData<Tag>` — это маркер нулевого размера. Он говорит компилятору «этот тип параметризован по `Tag`», при этом значение `Tag` нигде не хранится в рантайме.

Зачем он вообще нужен: если просто написать `struct Id<Tag> { raw: u64 }` без `PhantomData`, компилятор скажет `error[E0392]: parameter Tag is never used`. `PhantomData<Tag>` — это техническая необходимость, чтобы `Tag` «считалось использованным», но без выделения памяти под него.

Что получаем:

- В памяти `Id<Order>` и `Id<Instrument>` — один и тот же `u64`, 8 байт, никакого оверхеда.
- На этапе компиляции они — разные типы. `Id<Instrument>` не передать туда, где ждут `Id<Order>`.
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

let instrument_id = InstrumentId::new(42);
cancel_order(instrument_id);
// error[E0308]: expected `Id<Order>`, found `Id<Instrument>`
```

Та же гарантия, что и у newtype в первой секции, но с одной generic-реализацией для всех маркеров.

### Расширение: phantom-тег для валют

Та же техника даёт канонический пример — деньги с валютой в типе. Внутри `Money` — десятичная сумма (тот самый `Decimal` из раздела про newtype), но валюта вынесена в phantom-параметр:

```rust
use rust_decimal::Decimal;

pub struct Money<Currency> {
    amount: Decimal,
    _currency: PhantomData<Currency>,
}

pub struct Usd;
pub struct Eur;

impl<Currency> std::ops::Add for Money<Currency> {
    type Output = Money<Currency>;
    fn add(self, rhs: Money<Currency>) -> Money<Currency> {
        Money { amount: self.amount + rhs.amount, _currency: PhantomData }
    }
}
```

`Add` определён только для одной и той же валюты — у обоих операндов `Currency` один.
Поэтому доллары с долларами складываются, а доллары с евро — нет:

```rust
let usd: Money<Usd> = Money::new(dec!(100));
let eur: Money<Eur> = Money::new(dec!(100));

let _ = usd + usd;   // ок
let _ = usd + eur;   // error[E0308]: expected `Money<Usd>`, found `Money<Eur>`
```

Классическая ошибка «сложили доллары с евро» отсекается на компиляции.
А в рантайме `Money<Usd>` и `Money<Eur>` — это всё те же байты `Decimal`, без всякого тега.

### Маркеры — какие типы брать

Маркеры (`Order`, `Instrument`, `Usd`) не несут данных и не должны существовать как значения — их роль чисто на уровне типа. Подходят два варианта:

**Unit struct** — `pub struct Order;`. Можно сконструировать значение типа `Order`, но это никому не нужно. Простой, привычный, дешёвый.

**Empty enum** — `pub enum Order {}` (uninhabited type из прошлого раздела). Значение `Order` создать нельзя в принципе. Полезно, когда маркеры публичные и хочется на уровне типа запретить пользователю API случайно объявить `let _: Order = ...;` или вернуть `Order` из функции.

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

**`PhantomData` не влияет на размер.** `size_of::<Id<Order>>()` равен `size_of::<u64>()` — 8 байт.
После компиляции от phantom не остаётся ничего.

**`#[derive(...)]` на `Id<Tag>` требует, чтобы `Tag` тоже его поддерживал.**
Если написать `#[derive(Debug, Clone, PartialEq)] struct Id<Tag> { ... }`, компилятор сгенерирует `impl<Tag: Debug + Clone + PartialEq> Debug for Id<Tag>` — а маркеры по умолчанию ни одного из этих трейтов не реализуют, и тесты упадут с `error[E0277]: Order doesn't implement Debug`. Самый простой выход — навесить те же derive-ы и на маркеры:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Order;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Instrument;
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

Многие процессы — жизненный цикл заявки, парсинг, TLS-handshake, БД-транзакция — это машины состояний с чётким порядком шагов.
Заявка проходит путь «черновик -> подана в стакан -> исполнена или отменена». Если хранить состояние во флагах и `Option`-ах:

```rust
pub struct Order {
    submitted: bool,
    cancelled: bool,
    filled: bool,
    exchange_id: Option<OrderId>,
}

impl Order {
    pub fn submit(&mut self) -> Result<(), OrderError> { /* присвоить exchange_id */ }
    pub fn cancel(&mut self) -> Result<(), OrderError> {
        if !self.submitted {
            return Err(OrderError::NotSubmitted);
        }
        /* ... */
    }
    pub fn fill(&mut self) -> Result<(), OrderError> {
        if !self.submitted || self.cancelled {
            return Err(OrderError::NotActive);
        }
        /* ... */
    }
}
```

Те же беды, что в ADT-секции с независимыми флагами:

- Можно вызвать `cancel` до `submit` — получим рантайм-ошибку, не compile-time;
- Можно вызвать `submit` повторно и выставить заявку дважды — это тоже рантайм;
- Можно вызвать `fill` после `cancel` — снова рантайм;
- А `cancelled: true` вместе с `filled: true` вообще представимо в типе, хотя заявка не может быть и отменена, и исполнена.

Компилятор все эти ошибки в дизайне пропускает. Они проявляются в тестах (если повезло) или в проде.

### Решение: каждое состояние — свой тип

Дадим каждому шагу процесса свой тип. Методы, допустимые в этом шаге, — в его `impl`-блоке. Переход на следующий шаг — это метод, который потребляет `self` и возвращает следующее состояние.

```rust
pub struct DraftOrder {
    instrument: InstrumentId,
    side: Side,
    price: Price,
    quantity: Quantity,
}

pub struct WorkingOrder {
    id: OrderId, // биржа присвоила идентификатор
    instrument: InstrumentId,
    side: Side,
    price: Price,
    quantity: Quantity,
}

pub struct FilledOrder {
    id: OrderId,
    price: Price,
    quantity: Quantity,
}

pub struct CancelledOrder {
    id: OrderId,
}

impl DraftOrder {
    pub fn new(instrument: InstrumentId, side: Side, price: Price, quantity: Quantity) -> Self {
        Self { instrument, side, price, quantity }
    }

    /// Постановка в стакан: биржа присваивает id и может отклонить заявку.
    pub fn submit(self) -> Result<WorkingOrder, RejectReason> {
        /* поход на биржу */
        Ok(WorkingOrder { id: assign_id(), /* ...перенос полей... */ })
    }
}

impl WorkingOrder {
    pub fn fill(self) -> FilledOrder {
        FilledOrder { id: self.id, price: self.price, quantity: self.quantity }
    }

    pub fn cancel(self) -> CancelledOrder {
        CancelledOrder { id: self.id }
    }
}
```

Что это даёт:

```rust
let order = DraftOrder::new(/* ... */);
order.cancel();
// error[E0599]: no method named `cancel` found for struct `DraftOrder`
```

`cancel` физически невозможно вызвать на черновике — отменять нечего, заявка ещё не подана, и метода в его `impl`-блоке нет. То же с `fill`: он есть только на `WorkingOrder`.

Нормальный поток выглядит цепочкой:

```rust
let draft = DraftOrder::new(/* ... */);
let working = draft.submit()?;   // биржа приняла, присвоила id
let filled = working.fill();     // либо working.cancel()
```

Каждый переход потребляет `self`. После `submit` черновик `moved` — повторно его не подать. А отменённая заявка — это `CancelledOrder`, и у него нет ни одного метода, который вернул бы её в стакан: «отменённый ордер, всё ещё висящий в стакане» из начала статьи в типе невыразим. Это «illegal states unrepresentable» в самой строгой форме: невалидный порядок шагов не выражается, и компилятор не пускает вызов метода вне его состояния.

### Phantom-вариант: один тип, разные состояния

Подход выше использует отдельные структуры. Альтернатива — одна структура с phantom-параметром. Она удобна, когда состояния делят почти все поля (как `WorkingOrder` и `FilledOrder` — id, инструмент, сторона, цена, объём):

```rust
use std::marker::PhantomData;

pub struct Order<State> {
    id: OrderId,
    instrument: InstrumentId,
    side: Side,
    price: Price,
    quantity: Quantity,
    _state: PhantomData<State>,
}

#[derive(Debug, Clone, Copy)]
pub struct Working;

#[derive(Debug, Clone, Copy)]
pub struct Filled;

impl Order<Working> {
    pub fn fill(self) -> Order<Filled> {
        Order {
            id: self.id,
            instrument: self.instrument,
            side: self.side,
            price: self.price,
            quantity: self.quantity,
            _state: PhantomData,
        }
    }
}

impl Order<Filled> {
    pub fn settle(self) { /* ... */ }
}
```

Phantom-вариант удобен, когда:

- Состояния делят значительную часть полей (как здесь);
- Хочется один общий тип для логирования / сериализации / Debug;
- Нужны generic-методы, работающие на всех состояниях сразу.

Разные структуры удобнее, когда:

- У каждого состояния свой набор данных, разный по форме;
- Начальное состояние содержит не то же, что последующие (как `DraftOrder` выше — у него ещё нет `id`, который появляется только после `submit`).

В реальном коде часто смешивают: черновик — отдельная структура, выставленная заявка и её терминальные состояния — generic с phantom-параметром.

### Хорошие практики

**Потребляйте `self`, а не `&mut self` в переходах.** `&mut self` оставляет старое состояние доступным — можно вызвать переход дважды или забыть про новое значение.
Потребление `self` гарантирует, что старое значение физически уничтожено и вызывающий обязан работать с новым:

```rust
// Плохо — &mut self оставляет draft доступным после перехода:
fn submit(&mut self) { /* ... */ }

// Хорошо — self передан по значению, draft больше не существует:
fn submit(self) -> Result<WorkingOrder, RejectReason> { /* ... */ }
```

**Маркеры состояний — публичные unit-struct-ы (или пустые enum-ы).**
Вызывающему нужно говорить «вот тебе `Order<Working>`», значит, маркер должен быть `pub`.
Те же derive-ы, что и у parent-struct (см. ловушку из phantom-раздела).

**Имена состояний — фразы, описывающие точку процесса.**
`WorkingOrder`, `FilledOrder`, `Connected` — лучше, чем `State2`, `Phase1`.
По типу должно читаться, в каком шаге процесса мы находимся.

**Не пытайтесь выразить через typestate всё подряд.**
Если состояний больше пяти-шести и переходы между ними произвольные — это лучше моделируется обычным ADT (`enum Status { ... }`).
Typestate хорошо подходит, когда есть линейный или почти линейный порядок шагов; для произвольного графа переходов проще `enum + match`.

**Состояния-«тупики» через uninhabited-маркеры.**
Если из состояния нет выхода (как `Filled` или `Cancelled` у заявки), маркер можно сделать пустым enum-ом — `pub enum Filled {}` (uninhabited type из своего раздела).
Это документирует на уровне типа, что состояние терминальное.

### Где встречается в реальности

- **Embedded HAL-крейты**: GPIO-пины с состояниями `Input`, `Output<PushPull>`, `Alternate<AF1>` — типичный typestate, который запрещает писать в пин, настроенный на чтение.
- **Builder-паттерны с обязательными полями**: крейты `typed-builder` и `bon` превращают struct в typestate-builder, где компилятор не пускает `.build()` до тех пор, пока все обязательные поля не заполнены.
- **БД-транзакции**: в `sqlx::Transaction` методы `commit` и `rollback` принимают `self` по значению — повторный вызов невозможен, потому что транзакция перемещена.

### Итог Части 1 и что дальше

Хорошая модель домена — это не та, где можно выразить всё подряд.
Скорее наоборот: та, в которой трудно сделать неправильную вещь.
Если модель строится вокруг примитивов и независимых полей, типы обычно позволяют больше, чем реально существует в предметной области.
Тогда ограничения начинают жить отдельно — в if-ах, документации, комментариях и негласных договорённостях вроде «этот метод не вызывай после cancel».
Получается разрыв: типы разрешают больше состояний, чем существует в модели.

Type-driven подход пытается этот разрыв закрыть.
Типы начинают описывать не только форму данных, но и правила игры: какие состояния возможны, какие комбинации допустимы, какие переходы разрешены.
У этого есть цена. Типов становится больше, модель — подробнее, а код многословнее.
Но сложность не исчезает — она просто переезжает: из runtime-проверок в систему типов.
Вместо «не забудь проверить этот кейс» появляется ошибка компиляции.
Вместо документации в духе «сюда передавай только валидное состояние» — интерфейс, который не даёт передать невалидное.

Во 2 части перейдём от состояний к контрактам: трейты, associated types и const generics — инструменты, которыми Rust позволяет задавать правила уже не для отдельных объектов, а для взаимодействия между компонентами системы.