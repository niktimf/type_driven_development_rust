# Type-driven development в Rust. Часть 2/5: задаём контракты между компонентами — traits, associated types, const generics

В части 1 типы отвечали за состояние: какие значения возможны, какие комбинации допустимы, в каком порядке идут переходы.
Но кроме данных в программе есть компоненты, которые общаются между собой: заявка уходит на биржу, котировки приходят с рынка и так далее.
У каждой такой границы есть контракт — что требуется от реализации на той стороне.

Возьмём код из части 1: `DraftOrder::submit` отправлял заявку «на биржу» — но на какую?
Там мы об этом умолчали: биржа была одна и абстрактная.
В жизни их много, и подключаются они по-разному:
у одной REST-API поверх HTTP, у другой FIX-сессия, третья вообще живёт в блокчейне, а четвёртая — локальный симулятор для бэктеста.
Но поведение у всех одно: принять заявку и вернуть либо идентификатор, либо отказ.

Дальше площадки в примерах называются по способу подключения — `RestExchange`, `FixExchange`, `OnChainExchange`.

## Трейты как контракты

Трейт — это набор сигнатур, которые тип обязуется реализовать.
Компилятор не соберёт `impl`, где метод пропущен или сигнатура разошлась с объявленной.
Поэтому код, которому нужно поведение, а не конкретный тип, может потребовать сам трейт — и работать с любым, кто его реализовал.

### Проблема: захардкоженная биржа

Пока биржа одна, всё просто — зовём её напрямую(завязываемся на реализацию):

```rust
struct RestOrderId(u64);            // идентификатор из тела ответа
enum HttpExchangeError { /* ... */ }  // статус и код отказа от площадки

fn place_order(order: DraftOrder, exchange: &RestExchange) -> Result<RestOrderId, HttpExchangeError> {
    exchange.submit_order(&order)
}
```

Появилась вторая — и простота кончилась.
Первое, что может прийти в голову это `enum` по биржам и `match` на каждом вызове:

```rust
enum ExchangeKind {
    Rest(RestExchange),
    Fix(FixExchange),
}

fn route(order: DraftOrder, exchange: &ExchangeKind) -> Result</* ??? */, /* ??? */> {
    match exchange {
        ExchangeKind::Rest(e) => e.submit_order(&order),
        ExchangeKind::Fix(e) => e.submit_order(&order),
    }
}
```

Сразу же сталкиваемся с проблемой на типе возврата:
REST-площадка вернёт свой числовой идентификатор и HTTP-ошибку, FIX-площадка — строковый идентификатор и код отказа из FIX-сообщения, а `match` обязан вернуть что-то одно.

Каждая новая биржа — правка этого `enum` и всех `match`-ей по нему.
Даже симулятор для бэктеста придётся вписать вариантом в боевой тип (иначе через `route` его не позвать), и каждый боевой `match` обязан будет знать про тестовый контур.

В части 1 `enum` отлично кодировал «одно из» для данных с закрытым набором форм: третьей стороны у заявки не бывает, а новый вариант `OrderType` и должен подсветить каждый `match`.
Здесь набор открытый и растущий: площадок сколько угодно, и часть подключений появится уже после того, как роутер написан.

### Решение: трейт

Вынесем поведение в трейт(завязываемся на интерфейс, а не реализацию) — контракт «умею принять заявку»:

```rust
trait ExchangeClient {
    fn submit_order(&self, order: &DraftOrder) -> Result<OrderId, ExchangeError>;
}
```

`OrderId` и `ExchangeError` здесь — заглушки, общие на все площадки.
В данном случае так писать неправильно, и в разделе про ассоциированные типы мы разберём почему,
а пока они нужны только чтобы у трейта была сигнатура.

Каждая площадка выполняет контракт по-своему:

```rust
struct RestExchange { /* http-клиент, ключи */ }
struct FixExchange { /* FIX-сессия */ }

impl ExchangeClient for RestExchange {
    fn submit_order(&self, order: &DraftOrder) -> Result<OrderId, ExchangeError> { /* ... */ }
}

impl ExchangeClient for FixExchange {
    fn submit_order(&self, order: &DraftOrder) -> Result<OrderId, ExchangeError> { /* ... */ }
}
```

А код, который ставит заявку, про конкретные биржи больше не знает.
Он работает с любым, кто выполнил контракт:

```rust
fn route<EC: ExchangeClient>(
    order: &DraftOrder,
    exchange_client: &EC,
) -> Result<OrderId, ExchangeError> {
    exchange_client.submit_order(order)
}
```

`<EC: ExchangeClient>` читается как «для любого `EC`, который реализует `ExchangeClient`».
Ни `match`, ни зашитого списка площадок.
Новая биржа — это новый `impl`, и `route` подхватит её без единой правки.
Симулятор — тоже просто `impl` в тестовом контуре, а не вариант, вшитый в боевой тип.

`DraftOrder::submit` из части 1 ходил «на биржу» абстрактно — теперь на входе клиент площадки, и площадка любая:

```rust
impl<Quote: Currency> DraftOrder<Quote> {
    pub fn submit<EC>(
        self,
        exchange_client: &EC,
    ) -> Result<WorkingOrder<Quote, EC::ExchangeOrderId>, RejectReason>
    where
        EC: ExchangeClient<Quote = Quote>,
    {
        let exchange_id = exchange_client.submit_order(&self)?;
        Ok(WorkingOrder::accepted(self, exchange_id))
    }
}
```

Тип идентификатора в результате задаёт площадка:
`WorkingOrder<Usd, RestOrderId>` для REST-подключения,
`WorkingOrder<Usd, FixOrderId>` для FIX.
`ClientOrderId` при этом остаётся тем же самым — он наш, и от площадки не зависит.

### Хорошие практики

Что стоит держать в голове, когда контракт уже выделен в трейт.

**Где писать bound: `impl Trait`, `<EC: Trait>` или `where`.**
Три записи эквивалентны, но критерии выбора все же есть:

```rust
// 1. Тип не назван — короче всего, но упомянуть его второй раз не получится.
fn route(order: DraftOrder, exchange_client: &impl ExchangeClient) -> Result<OrderId, ExchangeError>

// 2. Тип назван — можно упомянуть его ещё раз и указать явно: route::<RestExchange>(...).
fn route<EC: ExchangeClient>(order: DraftOrder, exchange_client: &EC) -> Result<OrderId, ExchangeError>

// 3. Bound-ы ушли вниз — сигнатура остаётся читаемой, когда требований много.
fn route<EC>(order: DraftOrder, exchange_client: &EC) -> Result<OrderId, ExchangeError>
where
    EC: ExchangeClient + Send + Sync + 'static,
```

`impl Trait` в аргументе хорош, пока тип нужен ровно один раз.
Как только требуется связать два аргумента одним типом (`fn compare<EC: ExchangeClient>(a: &EC, b: &EC)`) или указать тип турбофишем(`::<>`) на вызове — нужен именованный параметр.
`where` обязателен, когда bound ставится не на сам параметр, а на выражение из него (`where Vec<EC::ExchangeOrderId>: Debug`).

**Bound-ы держите минимальными.**
Каждое требование в сигнатуре — часть контракта.
Лишний `Clone` в `route` не мешает сегодня, а завтра запретит подключить площадку, чей клиент держит неклонируемое соединение.
Разница в цене ошибки такая: новый bound на трейте ломает тех, кто его реализует, новый bound на функции — тех, кто её зовёт.

**Параметр типа расползается сам.**
Дальше в статье мы добавим в `Price` валюту котировки: `Price<Quote>`.
Одна правка в одном типе. Но все что содержит Price, тоже получает параметр `Quote`, а именно: `OrderType<Quote>`, `DraftOrder<Quote>`, `InstrumentSpec<Quote>`, `Level<Quote>`.
Итого пять типов вместо одного, и это никак не обойти: параметр обязан стоять везде, где лежит описываемое им значение.
Здесь он окупается — сложить доллары с евро больше не выйдет, но мы получили усложнение кода.

**Дефолтные методы расширяют контракт, не ломая реализации.**
Метод с телом в трейте — это поведение по умолчанию, которое реализация может переопределить:

```rust
trait ExchangeClient {
    fn submit_order(&self, order: &DraftOrder) -> Result<OrderId, ExchangeError>;

    /// Пакетная постановка. По умолчанию — просто цикл по одиночным.
    fn submit_batch(&self, orders: &[DraftOrder]) -> Vec<Result<OrderId, ExchangeError>> {
        orders.iter().map(|order| self.submit_order(order)).collect()
    }
}
```

У REST-площадки обычно есть отдельный batch-эндпоинт — она переопределит `submit_batch` и отправит всё одним запросом.
У FIX-сессии такого эндпоинта нет, заявки уходят по одной — её устраивает цикл, и в `impl` про `submit_batch` можно не вспоминать.
Так контракт делится на обязательный минимум (`submit_order`) и производные удобства.

К `submit_batch` мы ещё вернёмся дважды: в разделе про ассоциированные типы у площадки появится лимит на размер пакета, а в разделе про const generics мы попробуем связать этот лимит с длиной массива — и столкнемся с проблемой.

**Статическая и динамическая диспетчеризация.**
`route<EC: ExchangeClient>` мономорфизируется: компилятор создаёт отдельную копию функции под каждую площадку, вызов идёт напрямую и может быть заинлайнен.
Платим размером бинарника и временем компиляции.

Но generic-параметр — это «один тип на всю функцию», и гетерогенную коллекцию так не собрать.
Роутеру же нужно держать все подключённые площадки разом, а их набор известен только из конфига:

```rust
/// Какая из подключённых площадок — ключ из конфига: "rest", "fix", "onchain".
pub struct ExchangeName(pub String);

struct Router {
    exchanges: HashMap<ExchangeName, Box<dyn ExchangeClient + Send + Sync>>,
}
```

`HashMap<ExchangeName, EC>` тут не подходит: это одна-единственная площадка, размноженная по ключам.
`Box<dyn ExchangeClient>` — «жирный» указатель: данные плюс таблица виртуальных функций, одна копия кода.

Не всякий трейт годится в `dyn`.
Требование называется dyn compatibility (до конца 2024-го термин звучал как object safety) и сводится к тому, влезает ли метод в vtable: generic-методы нельзя, `fn new() -> Self` нельзя, ассоциированные константы нельзя, ассоциированные типы обязаны быть указаны прямо в `dyn Trait<Item = ...>`.
Метод, который в vtable не влезает, можно из неё исключить, пометив `where Self: Sized`, — тогда сам трейт остаётся dyn-совместимым, а метод доступен только по конкретному типу.

Выбирать между `dyn` и generic по производительности почти никогда не нужно: разница в единицы наносекунд на вызов, а до биржи идёт сетевой round-trip в миллисекундах.
Выбирают по устройству кода: набор реализаций известен на этапе компиляции — generic, набор собирается в рантайме — `dyn`.

**Одна реализация на пару «трейт + тип».**
`impl ExchangeClient for RestExchange` в программе может быть ровно один — это правило когерентности.
А вариантов работы с одной и той же площадкой обычно больше: боевой контур, песочница биржи со своим адресом и своим пространством идентификаторов, детерминированный симулятор для бэктеста.
Все трое — «та же площадка», но `impl` на них один. Обходят это отдельными типами (`RestExchange`, `RestSandbox`) или обёрткой-newtype что в части 1, только теперь newtype нужен, чтобы реализовать второй `impl`.
Во что это выливается, когда вариантов становится много, — в разделе про CGP в конце статьи.

**Async в контракте.**
Настоящий `submit` ходит по сети, то есть асинхронен.
`async fn` в трейтах работает со стабильного Rust 1.75, но у него два свойства, о которых стоит знать заранее:

```rust
trait ExchangeClient {
    async fn submit_order(&self, order: &DraftOrder) -> Result<OrderId, ExchangeError>;
}
```

Во-первых, такой трейт не dyn-совместим — `Box<dyn ExchangeClient>` не собрать (нужен `#[async_trait]`, который боксирует каждый future, или `dynosaur`).
Во-вторых, у возвращаемого future нет bound-а `Send`, и generic-код не сможет отправить его в `tokio::spawn`.
Лечится либо явной записью `fn submit_order(&self, order: &DraftOrder) -> impl Future<Output = Result<OrderId, ExchangeError>> + Send`, либо макросом `trait_variant::make`, который генерирует `Send`-версию трейта рядом с исходной.

**Sealed trait — контракт, который нельзя реализовать снаружи.**
Для `ExchangeClient` открытость и была целью: чужая площадка должна подключаться без правок нашего кода.
Но бывает наоборот. Вспомним `Money<Currency>` из части 1: параметр там ничем не ограничен, и `Money<OrderId>` — «сумма в идентификаторах заявки» — спокойно компилируется.
Валют же в домене конечный список, и новое подключение его не расширяет.

Ставим bound и запечатываем трейт: реализовать его снаружи нельзя, потому что нельзя реализовать его приватный супертрейт.

```rust
mod sealed {
    pub trait Sealed {}
}

pub trait Currency: sealed::Sealed {
    const CODE: &'static str;
    const MINOR_UNITS: u32;   // сколько знаков после запятой: USD — 2, EUR — 2
}

pub struct Usd;
pub struct Eur;

impl sealed::Sealed for Usd {}
impl Currency for Usd {
    const CODE: &'static str = "USD";
    const MINOR_UNITS: u32 = 2;
}
// Аналогично Eur

pub struct Money<Quote: Currency> {
    amount: Decimal,
    _quote: PhantomData<Quote>,
}

let broken: Money<OrderId> = /* ... */;
// error[E0277]: the trait bound `OrderId: Currency` is not satisfied
```

Заодно случилось кое-что важное: фантомный параметр перестал быть чисто фантомным.
Через трейт у него появились данные — `Quote::CODE`, `Quote::MINOR_UNITS` — доступные в рантайме, хотя значения типа `Quote` не существует:

```rust
impl<Quote: Currency> fmt::Display for Money<Quote> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Точность в форматтере, а не round_dp: тот округляет, но не
        // дополняет нулями — 100 так и осталось бы «100», а не «100.00».
        write!(f, "{:.*} {}", Quote::MINOR_UNITS as usize, self.amount, Quote::CODE)
    }
}
```

Запечатывайте контракты, за которые отвечаете сами (там, где важно держать инвариант во всех реализациях сразу и иметь право дописывать методы, не ломая чужой код), и оставляйте открытыми точки расширения.

### В библиотеках

- В `std` `Iterator` — один обязательный `next` и несколько десятков дефолтных методов поверх него: `map`, `filter`, `fold` и так далее.
- `SliceIndex` — sealed-трейт в `std`: он описывает, чем можно индексировать срез (`usize`, `Range`, `RangeFrom`), и закрыт от внешних реализаций через приватный супертрейт.

Наш `ExchangeClient` пока обещает всем площадкам один `OrderId` и один `ExchangeError`, но на практике у каждой площадки свои типы.

## Ассоциированные типы

Ассоциированный тип объявлен в трейте, но выбирает его реализация.
Самый распространенный пример — в `std`:

```rust
trait Iterator {
    type Item;
    fn next(&mut self) -> Option<Self::Item>;
}
```

`Item` объявлен один раз, а чему он равен — решает каждый `impl` отдельно: у итератора по вектору заявок это `Order`, у итератора по строкам — `&str`.
Трейт при этом остаётся однопараметрическим: `Iterator`, а не `Iterator<Item>`.

### Проблема: у каждой площадки свои типы

REST-площадка возвращает числовой идентификатор из тела ответа, а DEX — 32-байтовый хеш транзакции.
У FIX-сессии идентификатор строковый: в FIX поля не именуются, а нумеруются, и идентификатор заявки лежит в поле 37.
Ошибки тоже разные: HTTP-статус и код от биржи, а у FIX номера причины отказа в поле 103.
Общий `OrderId` на всех — это выбор из двух, и оба плохие.
Либо самый широкий тип — `String`, и тогда каждая площадка сводит свой идентификатор к тексту: число к цифрам, хеш к hex и контракт снова держится на договорённостях.
Либо тип одной из площадок, и тогда остальные контракты выполнить не смогут: 32 байта в `u64` не помещаются и так далее.

Первое, что приходит в голову, — вынести типы в параметры трейта:

```rust
trait ExchangeClient<Id, Error> {
    fn submit_order(&self, order: &DraftOrder) -> Result<Id, Error>;
}
```

Проблемы начинаются сразу за границей трейта.
Параметры протекают в каждую функцию, которая работает с клиентом площадки, и тянутся дальше через все слои:

```rust
fn route<EC, Id, Error>(order: DraftOrder, exchange_client: &EC) -> Result<Id, Error>
where
    EC: ExchangeClient<Id, Error>,
```

Но хуже другое: типы выбирает вызывающий, а не площадка.
Ничто не мешает написать для одной площадки два `impl`-а — `ExchangeClient<RestOrderId, _>` и `ExchangeClient<String, _>`, — и тогда обычный `exchange_client.submit_order(&order)` станет неоднозначным: компилятор не сможет понять, какой именно `impl` имелся в виду (`error[E0282]`), и турбофиш(`::<>`) придётся писать на каждом вызове.
А ведь у площадки формат идентификатора ровно один — это её свойство, а не параметр, который кто-то подбирает снаружи.

### Решение: тип принадлежит реализации

Тип объявлен в трейте один раз, а подставляет его каждая площадка в своём `impl` блоке.

```rust
trait ExchangeClient {
    type ExchangeOrderId;
    type Error;

    fn submit_order(&self, order: &DraftOrder) -> Result<Self::ExchangeOrderId, Self::Error>;

    /// Отменить можно только тем id, который вернула площадка.
    fn cancel_order(&self, id: Self::ExchangeOrderId) -> Result<(), Self::Error>;
}


struct RestOrderId(u64);
struct HttpExchangeError { /* ... */ }

impl ExchangeClient for RestExchange {
    type ExchangeOrderId = RestOrderId;
    type Error = HttpExchangeError;

    fn submit_order(&self, order: &DraftOrder) -> Result<RestOrderId, HttpExchangeError> { /* ... */ }
}


struct FixOrderId(String);
struct FixReject { reason: u16, text: String }

impl ExchangeClient for FixExchange {
    type ExchangeOrderId = FixOrderId;
    type Error = FixReject;

    fn submit_order(&self, order: &DraftOrder) -> Result<FixOrderId, FixReject> { /* ... */ }
}
```
Для `Id` используется newtype над внутренним представлением, чтобы не путать идентификаторы разных площадок.
`Error` — структура с полями, которые площадка возвращает в случае отказа.

`route` снова однопараметрический, а типы результата берутся у самой площадки:

```rust
fn route<EC: ExchangeClient>(
    order: &DraftOrder,
    exchange_client: &EC,
) -> Result<EC::ExchangeOrderId, EC::Error> {
    exchange_client.submit_order(order)
}
```

Второго `impl`-а `ExchangeClient for RestExchange` с другим идентификатором теперь не существует в принципе: `type ExchangeOrderId` фиксируется один раз вместе с реализацией.
Неоднозначности нет, турбофиш(`::<>`) не нужен, а сигнатура `route` не растёт от того, что у площадок разные внутренности.

### Входные и выходные типы

Разница между параметром трейта и ассоциированным типом — это разница между входом и выходом.
Параметр — вход: его выбирает вызывающий, и у одного типа таких реализаций может быть много.
`String` реализует `From<&str>`, `From<char>`, `From<Box<str>>` — десяток входов, и это нормально.
Ассоциированный тип — выход: его выбирает реализация, и он ровно один. `Iterator::Item` у `std::vec::IntoIter<Order>` — это `Order`, и другого варианта быть не может.
Проверочный вопрос при проектировании контракта: «может ли у одного типа быть два разных ответа?» Да — параметр. Нет — ассоциированный тип.

Обе роли встречаются в одном трейте — например, в `std::ops::Mul`:

```rust
pub trait Mul<Rhs = Self> {
    type Output;
    fn mul(self, rhs: Rhs) -> Self::Output;
}
```

`Rhs` — вход: умножать можно на разное.
`Output` — выход: для конкретной пары «что умножаем» и «на что» результат один.

Это как раз то, чего не хватало части 1.
Там номинал заявки считала функция `notional(price, quantity) -> Decimal`: типы, которые мы так старательно строили, схлопывались в голое число ровно там, где речь про деньги.
Заведём инструменту валюту котировки — тем же phantom-приёмом, что и у `Money<Currency>`, — и результат перестанет её терять:

```rust
pub struct Price<Quote> {
    amount: Decimal,
    _quote: PhantomData<Quote>,
}

impl<Quote: Currency> Mul<Quantity> for Price<Quote> {
    type Output = Money<Quote>;

    fn mul(self, quantity: Quantity) -> Money<Quote> {
        Money::new(self.amount * quantity.amount())
    }
}

let notional: Money<Usd> = price * quantity;   // валюта видна в типе результата
let nonsense = price * price;
// error[E0277]: the trait bound `Price<Usd>: Mul<Price<Usd>>` is not satisfied
```

Умножение цены на цену не запрещали отдельно — просто такого `impl`-а нет, а значит, операции не существует.
`InstrumentSpec` при этом становится `InstrumentSpec<Quote>`: спецификация инструмента и раньше знала валюту котировки, теперь эта валюта проставляется в каждую цену, которую она выпускает через smart constructor.

### Когда ассоциированному типу нужен свой параметр: GAT

Стакан живёт в буфере, который постоянно перезаписывается новыми снимками.

Уровень стакана — цена и объём на ней:
```rust
pub struct Level {
    price: Price<Usd>,
    quantity: Quantity,
}
```

Хочется отдавать наружу итератор по уровням, ничего не копируя:
```rust
trait MarketDataFeed {
    type Levels: Iterator<Item = Level>;
    fn bids(&self) -> Self::Levels;
}
```

Так не выйдет: `Self::Levels` — один фиксированный тип, и привязать его к времени жизни `&self` негде.
Итератор придётся делать владеющим, то есть копировать буфер на каждый вызов.

Generic associated types (GAT, стабильны с Rust 1.65) снимают ограничение — у ассоциированного типа появляются собственные параметры:

```rust
trait MarketDataFeed {
    type Levels<'a>: Iterator<Item = &'a Level>
    where
        Self: 'a;

    fn bids(&self) -> Self::Levels<'_>;
}

/// Буфер, куда фид складывает последний снимок стакана.
struct BookFeed {
    bids: Vec<Level>,
    asks: Vec<Level>,
}

impl MarketDataFeed for BookFeed {
    type Levels<'a> = std::slice::Iter<'a, Level> where Self: 'a;

    fn bids(&self) -> Self::Levels<'_> {
        self.bids.iter()
    }
}
```

Теперь тип итератора зависит от времени жизни заимствования: копий нет, а компилятор следит, чтобы итератор не пережил буфер.
GAT — заметная часть фундамента, на котором стоит CGP из финала статьи.

### Хорошие практики

**Bound-ы на самом ассоциированном типе, а не на каждой функции.**
Требования, общие для всех площадок, объявляют один раз в трейте:

```rust
trait ExchangeClient {
    type OrderId: Debug + Clone + Send + Sync + 'static;
    type Error: std::error::Error + Send + Sync + 'static;
}
```

`Error: std::error::Error + Send + Sync + 'static` — практически стандартный набор: с ним ошибка складывается в `Box<dyn Error>` и `anyhow`, а `thiserror` может взять её в `#[source]`.
Но каждый bound — это ограничение для тех, кто трейт реализует, и добавить его потом больно.

**Ассоциированные константы — то же самое, но для значений.**
Мы их уже видели у `Currency`.

В контракте площадки они выглядят так:
```rust
trait Exchange {
    const NAME: &'static str;
    const MAX_BATCH: usize;
}

impl Exchange for RestExchange {
    const NAME: &'static str = "binance";
    const MAX_BATCH: usize = 5;
}
```

Значение известно на этапе компиляции и доступно без экземпляра — `RestExchange::MAX_BATCH`, `EC::NAME` в generic-коде.
Отдельным трейтом это вынесено не случайно: ассоциированная константа делает трейт dyn-несовместимым, а `ExchangeClient` нам ещё понадобится в `Box<dyn ...>`.

**Ассоциированные типы и `dyn`: стирать типы надо осознанно.**
Как только у трейта появились ассоциированные типы, в `dyn`-форме их придётся назвать: `Box<dyn ExchangeClient<ExchangeOrderId = ..., Error = ...>>`.
А типы у площадок как раз разные — поэтому роутер из раздела про трейты так просто не собрать.

Выход — не размазывать унификацию по всему коду, а сделать её одним явным тонким слоем на границе роутера:

```rust
/// Идентификатор после стирания: какая площадка плюс её id, приведённый к строке.
pub struct ErasedOrderId {
    exchange: ExchangeName,
    raw: Box<str>,
}

/// Одна ошибка на всех: точные типы площадок сюда уже не помещаются.
pub enum RouterError {
    UnknownExchange(ExchangeName),
    Rejected { exchange: &'static str, detail: String },
    UnparseableOrderId(String),   // понадобится ниже, на отмене
}

/// Приводит типы конкретной площадки к общим типам роутера.
pub struct Erased<EC>(pub EC);

impl<EC> ExchangeClient for Erased<EC>
where
    EC: ExchangeClient + Exchange,
    EC::ExchangeOrderId: Display,
    EC::Error: Into<RouterError>,
{
    type ExchangeOrderId = ErasedOrderId;   // «какая площадка + её строковый id»
    type Error = RouterError;

    fn submit_order(&self, order: &DraftOrder) -> Result<ErasedOrderId, RouterError> {
        self.0
            .submit_order(order)
            .map(|id| ErasedOrderId::new(EC::NAME, id.to_string()))
            .map_err(Into::into)
    }
}
```

Дальше `HashMap<ExchangeName, Box<dyn ExchangeClient<ExchangeOrderId = ErasedOrderId, Error = RouterError> + Send + Sync>>` собирается без вопросов.
Точные типы площадки живут внутри её модуля, стирание происходит в одном месте, и на ревью видно, где именно система теряет информацию.

**Стирание работает в одну сторону.**
Заявку поставили, получили `ErasedOrderId` — по сути строку.
Теперь её надо отменить, а `cancel_order` у площадки принимает родной идентификатор: `RestOrderId` у REST, `TxHash` у DEX.
Обратной операции к `Display` в Rust нет — из `"42"` не собрать `RestOrderId`, из hex-строки не собрать 32 байта.
Нужно добавить bound, который гарантирует, что площадка умеет разбирать свои идентификаторы обратно в родной тип, в данном случае это трейт `TryFrom<ErasedOrderId>`.

```rust
impl<EC> ExchangeClient for Erased<EC>
where
    EC: ExchangeClient + Exchange,
    EC::ExchangeOrderId: Display + TryFrom<ErasedOrderId>,
    EC::Error: Into<RouterError>,
{
    fn cancel_order(&self, id: ErasedOrderId) -> Result<(), RouterError> {
        let native = EC::ExchangeOrderId::try_from(id)
            .map_err(|_| RouterError::UnparseableOrderId)?;
        self.0.cancel_order(native).map_err(Into::into)
    }
}
```

Площадка, чей идентификатор не разбирается обратно, за роутер теперь не пройдёт — компилятор скажет об этом на месте.

**Не заводите ассоциированный тип там, где тип один на всех.**
`DraftOrder` на входе `submit_order` одинаков для любой площадки — так и пишем его прямо.
Обобщение стоит читаемости и усложняет код.

### В библиотеках

- `tower::Service` — приём, развёрнутый до экосистемы: `type Response`, `type Error` и, что интереснее, `type Future` — асинхронный результат тоже выбирает реализация.
На этом держится весь middleware-слой: `Layer` оборачивает один `Service` в другой, и таймауты, ретраи, лимиты собираются из типов.
- `sqlx::Database` описывает СУБД целиком: `type Connection`, `type Row`, `type ValueRef<'r>` — последний уже GAT, в продакшене и на стабильном Rust.
До версии 0.9 GAT-ом был и `Arguments<'q>`, выгода — отложенное заимствование: аргументы держат бинд-значения по ссылке до самого исполнения.
Цена — лайфтайм: `'q` расползался по сигнатурам до `Query<'q>`, и динамически собранный запрос нельзя было сделать `Query<'static>`.
В 0.9 решили, что цена выше выгоды: заимствование обменяли на владеющий буфер (значение кодируется прямо в `add`), и GAT исчез.
А `ValueRef<'r>` остался — zero-copy чтение значения из строки результата: то заимствование, ради которого GAT и заводят.

Стакан на десять уровней и стакан на пять для компилятора один и тот же тип, а для стратегии — разные, но как их отличить?

## Const generics

Обычный generic-параметр — это тип: `Vec<T>`, `Option<T>`.
Const-параметр — значение, известное на этапе компиляции:

```rust
struct Buffer<const N: usize> {
    data: [u8; N],
}
```

`Buffer<64>` и `Buffer<128>` — разные типы, и длина у них лежит не в поле, а в самом типе.
Значит компилятор знает её на каждом обращении: границы можно не проверять, цикл — развернуть.

### Проблема: глубина стакана известна заранее, но хранится в рантайме

Стратегии смотрят на верхние уровни стакана — те самые `Level`, которые фид отдавал итератором в предыдущем разделе.
Сколько уровней нужно — зависит от стратегии.
Маркет-мейкеру хватает лучшей цены с каждой стороны: один уровень.
Тому, кто ловит дисбаланс объёмов или айсберги, нужно десять или двадцать — на одном уровне дисбаланса не видно.
Фид под каждую подписку свой: top-of-book отдаёт один уровень, L2 — пять, десять или двадцать.

Обе стратегии находятся в одном процессе, и снимок обеим приходит одного и того же типа.
При этом глубина — свойство алгоритма, а не потока данных: код дисбаланса написан под десять уровней — индексы, веса, пороги — и под пять его не подставить.
Она известна в момент написания кода, задолго до первого байта данных; подписке в конфиге остаётся только совпасть с ней.
Число, известное на этапе компиляции, лежит в рантайме и ничего не гарантирует.

Обычная запись про это не знает:

```rust
pub struct OrderBookSnapshot {
    bids: Vec<Level>,
    asks: Vec<Level>,
}
```

Здесь несколько проблем:

- длины нет в типе: маркет-мейкерский снимок на один уровень уйдёт в стратегию, которой нужно десять, — и это выяснится в рантайме;
- `bids.len() != asks.len()` представимо: в типе длины сторон ничем не связаны, хотя подписка с фиксированной глубиной обещает её обеим сторонам;
- две аллокации на снимок, а снимки идут тысячами в секунду;
- компилятор не знает длину: индексация — проверка границ и ветка паники, полностью развернуть цикл не выйдет.

### Решение: размер как параметр типа

Добавим длину в тип:

```rust
pub struct OrderBook<const DEPTH: usize> {
    bids: [Level; DEPTH],
    asks: [Level; DEPTH],
}

pub type TopOfBook = OrderBook<1>;
pub type L2Book = OrderBook<10>;
```

Что получаем:
- `OrderBook<1>` и `OrderBook<10>` — разные типы.
Снимок на пять уровней в функцию, которой нужны десять, не передать: `error[E0308]: expected OrderBook<10>, found OrderBook<5>`.
- `bids` и `asks` одной длины по построению — рассогласовать их нельзя.
- Данные лежат в самой структуре, без кучи; размер известен на этапе компиляции.
- Реализация одна на все глубины:

```rust
impl<const DEPTH: usize> OrderBook<DEPTH> {
    pub const fn depth(&self) -> usize {
        DEPTH
    }

    pub fn spread(&self) -> Decimal {
        self.asks[0].price.amount() - self.bids[0].price.amount()
    }
}
```

### Ограничение, которое стоит поставить сразу

`spread` выше молча предполагает, что уровень хотя бы один.
Но `OrderBook<0>` — валидный тип, и на нём тот же код упадёт.
Написать `where DEPTH >= 1` на стабильном Rust нельзя; не поможет и `NonZeroUsize` — в const-параметрах разрешены только целые числа, `bool` и `char`, а длине массива нужен `usize`.
Зато условие можно свести к обычному trait bound:

```rust
pub struct BookDepth<const N: usize>;

pub trait SupportedDepth {}
impl SupportedDepth for BookDepth<1> {}
impl SupportedDepth for BookDepth<5> {}
impl SupportedDepth for BookDepth<10> {}

impl<const DEPTH: usize> OrderBook<DEPTH>
where
    BookDepth<DEPTH>: SupportedDepth,
{
    pub fn new(bids: [Level; DEPTH], asks: [Level; DEPTH]) -> Self {
        Self { bids, asks }
    }
}

let empty = OrderBook::<0>::new([], []);
// error[E0277]: the trait bound `BookDepth<0>: SupportedDepth` is not satisfied
```

Поля приватные, других конструкторов нет — значит, `OrderBook<0>` не собрать, и `spread` безопасен по построению.
Это тот же smart constructor из части 1, только проверяет он не значение, а параметр типа.
Сам `SupportedDepth` уместно запечатать: список глубин дискретен по построению — под каждую в системе написана стратегия.
Площадка по FIX-запросу отдаст хоть семь уровней (`MarketDepth=7` легален), но под тип `OrderBook<7>` кода нет.

Хочется вообще убрать список и забрать глубину из контракта фида — ассоциированной константой, как `MAX_BATCH` у `Exchange`.
Почему так не выйдет, видно ниже на `submit_batch`.

### Проблема: арифметика в параметрах

Дефолтный метод из раздела про трейты брал срез и ничего не знал про лимиты.
Лимит появился в предыдущем разделе — `Exchange::MAX_BATCH`, ассоциированная константа.
Логично свести их вместе: выразить пакет массивом и проверить размер типом.

```rust
trait ExchangeClient: Exchange {
    fn submit_batch<const N: usize>(
        &self,
        orders: [DraftOrder; N],
    ) -> [Result<Self::ExchangeOrderId, Self::Error>; N]
    where
        N <= Self::MAX_BATCH;   // так нельзя
}
```

Такой записи в Rust нет — `where` принимает только баунды.
На nightly условие выражают следующим образом: `generic_const_exprs` разрешает арифметику над const-параметрами (`[T; N + 1]`, `N * 2`) и позволяет спрятать сравнение в баунд-тип; как это выглядит — в части 4.

На стабильном есть обходной путь через const-блок:

```rust
pub fn submit_batch_typed<EC, const N: usize>(
    exchange_client: &EC,
    orders: [DraftOrder; N],
) -> [Result<EC::ExchangeOrderId, EC::Error>; N]
where
    EC: ExchangeClient + Exchange,
{
    const { assert!(N <= EC::MAX_BATCH, "batch exceeds exchange limit") };
    orders.each_ref().map(|order| exchange_client.submit_order(order))
}
```

Обратиться к ассоциированной константе типа-параметра внутри `const`-блока стабильный Rust позволяет — nightly для этого не нужен.
Но проверка получается не той, на которую хочется рассчитывать. Вот что видно на компиляторе, если инстанцировать функцию с `N` больше лимита:

```
$ cargo check      # проходит
$ cargo build
error[E0080]: evaluation panicked: batch exceeds exchange limit
   |
   = note: while instantiating `submit_batch_typed::<RestExchange, 10>`
```

Это post-monomorphization error: она возникает не на сигнатуре, а при инстанцировании функции, и `cargo check` её действительно не видит — нужна полная сборка, и только если этот код вообще дошёл до кодогенерации.
В одном воркспейсе это терпимо: сборка своя, и промах виден хоть и поздно, но до релиза.

Отсюда общее правило: то, что выражается через bound (как `SupportedDepth` выше), лучше выражать через bound — ошибка тогда приходит на месте вызова и с внятным текстом.

### Хорошие практики

**Только целые числа, `bool` и `char`.**
`&'static str`, `f64`, свои структуры в const-параметры не годятся.
Структуры и строки доступны на nightly (`adt_const_params`; строкам нужен ещё `unsized_const_params`), а `f64` — нигде, и это принципиально: у флоатов нет структурного равенства (`NaN != NaN`, а `0.0 == -0.0` при разных битах), так что компилятор не смог бы решить, один ли тип `Buffer<X>` перед ним.
Поэтому `NAME: &'static str` у нас ассоциированная константа, а не параметр типа.

**Значение по умолчанию можно задать на определении типа.**
`pub struct OrderBook<const DEPTH: usize = 10>` — дальше `OrderBook` без аргумента означает `OrderBook<10>`.

**Помните про мономорфизацию.**
Каждое значение `DEPTH` — отдельная копия всего кода, который по нему обобщён.
Параметризовать по глубине стакан — нормально; параметризовать по ней весь пайплайн стратегии — значит размножить пайплайн.

**Где фичи, а где const-параметры.**
Фича даёт одно значение на сборку, const-параметр — сколько угодно типов в одной.
Маркет-мейкер с `OrderBook<Usd, 1>` и детектор дисбаланса с `OrderBook<Usd, 10>` в одном процессе — работа для const-параметра, фичами такое не собрать.
Фичи аддитивны и объединяются по всему графу сборки: если два крейта попросят у общего стакана `depth5` и `depth10` разом, cargo включит обе, и два `#[cfg]`-определения одной константы не соберутся; отключить чужую фичу нельзя.
Фичи используются в другом случае — «один бинарь на стратегию»: кодовая база одна, различия стратегий лежат небольшими кусками под фичами, и каждая стратегия собирается своим бинарём со своим набором `--features`.
Конфигурация здесь не динамическая, а флагами компиляции: внутри сборки она одна, компилятор инлайнит её насквозь, и аддитивности негде сработать — наборы фич не встречаются в одном графе. В HFT так и делают.
В сборку должна попасть ровно одна фича стратегии; несовместимые комбинации закрывают `compile_error!`.
Каноническое же применение фич — аддитивные необязательные части: поддержка `serde`, ещё один протокол.

**Не тащите в тип то, что действительно динамическое.**
Глубина стакана зашита в код стратегии — ей место в типе.
Число заявок, вычитанных из очереди за итерацию, — нет: это рантайм-величина использовать `Vec` нормально.

**Иногда достаточно конкретного массива.**
Если польза от параметра только в том, чтобы «не забыть длину», а обобщать нечего, `[Level; 10]` в конкретной структуре читается лучше любого const-параметра.

### В библиотеках

- В `std` const generics видны в массивах: до Rust 1.47 трейты для `[T; N]` генерировались макросом до 32 элементов, потом появился общий `impl<T, const N: usize>` — std сделал это на внутренней реализации фичи, раньше, чем она стала доступна пользователям в 1.51.
Отсюда `IntoIterator` для массивов, `TryFrom<Vec<T>> for [T; N]`, `array::from_fn`.
- `generic-array` и `typenum` — как это делали до 1.51, кодируя числа типами. Техника не умерла: в части 3 мы вернёмся к ней, когда будем строить на уровне типов уже не числа, а списки.
- Post-monomorphization ошибки — причина прятать const-блоки от публичного API: промах найдёт не автор крейта, а тот, кто его подключил и первым инстанцировал функцию с неудачным `N`.

У всех трех приёмов есть ограничение — «один `impl` на пару трейт + тип».
Существует проект, построенный вокруг того, чтобы его обойти.

## CGP

Смотреть на него полезно, даже если внедрять не собираетесь: он показывает, чего обычные трейты не умеют.

### Проблема: одна реализация на тип

Про правило когерентности мы уже говорили: `impl ExchangeClient for RestExchange` в программе ровно один.
Пока реализация одна, это все нормально. Но чем крупнее система, тем чаще одно и то же поведение хочется иметь в нескольких вариантах:

- отправка на одну и ту же площадку — по REST и по WebSocket;
- ошибки — `anyhow` в приложении, свой типизированный `enum` в библиотеке, `()`-заглушка в тестах;
- сама биржа — реальная, песочница, детерминированный симулятор для бэктеста.

Обычный ответ — newtype-обёртки (`RestExchange`, `RestSandbox`, `WsExchange`) плюс generic-параметры, которые расползаются по сигнатурам.
Плюс ассоциированные типы фиксируются вместе с `impl`-ом: `type Error` у площадки один и тот же и в проде, и в тесте.

### Решение: контракт отдельно, реализация отдельно, сборка отдельно

CGP (Context-Generic Programming) разделяет трейт на две части: consumer-трейт, который зовёт прикладной код, и provider-трейт, который реализуют.
Реализации становятся самостоятельными типами, а не «привязками к контексту», и их может быть сколько угодно.
Выбор конкретной реализации выносится в отдельную декларацию — по одной на контекст (приложение, тест, деплой):

```rust
use cgp::prelude::*;

// Абстрактный тип идентификатора: у каждого контекста будет свой.
#[cgp_type]
pub trait HasOrderIdType {
    type OrderId;
}

// Consumer-трейт — то, что видит прикладной код.
#[cgp_component(OrderSubmitter)]
pub trait CanSubmitOrder: HasOrderIdType + HasErrorType {
    fn submit_order(&self, order: &DraftOrder<Usd>) -> Result<Self::OrderId, Self::Error>;
}
```

Реализации — отдельные типы, а не `impl` «для контекста».
Каждый провайдер реализует сгенерированный provider-трейт ровно один раз, поэтому когерентности нечего нарушать:

```rust
struct RestOrderId(u64);

pub struct SubmitViaRest;

#[cgp_provider]
impl<Context> OrderSubmitter<Context> for SubmitViaRest
where
    Context: HasOrderIdType<OrderId = RestOrderId> + HasErrorType<Error = HttpExchangeError>,
{
    fn submit_order(
        _context: &Context,
        order: &DraftOrder<Usd>,
    ) -> Result<RestOrderId, HttpExchangeError> {
        /* HTTP-запрос на площадку */
    }
}

/// Симулятор считает заявки у себя, поэтому и идентификатор у него свой.
struct SimOrderId(u64);

pub struct SubmitToSimulator;

#[cgp_provider]
impl<Context> OrderSubmitter<Context> for SubmitToSimulator
where
    Context: HasOrderIdType<OrderId = SimOrderId> + HasErrorType<Error = Infallible>,
{
    fn submit_order(_context: &Context, order: &DraftOrder<Usd>) -> Result<SimOrderId, Infallible> {
        /* локальный матчинг для бэктеста */
    }
}
```

В этих сигнатурах стоит заметить две вещи.
Метод провайдера принимает `context: &Context`, а не `&self`: сам провайдер — тип нулевого размера, состояния он не держит, всё нужное лежит в контексте.
И `type Error` у симулятора — `Infallible`, пустой тип из части 1.

Половину имён в этих сниппетах вы не найдёте в исходнике — их пишет макрос.
`#[cgp_component(OrderSubmitter)]` генерирует и сам provider-трейт `OrderSubmitter`, и маркер `OrderSubmitterComponent`, по которому контекст выбирает реализацию.
`HasErrorType` и `UseType` приходят из `cgp::prelude`.
Из-за этого ошибки компилятора приходят про типы, которых вы не писали; что с этим делать рассмотрим ниже.

Дальше каждый контекст в одном месте объявляет, кто за что отвечает:

```rust
/// Контекст — это тип, который держит зависимости приложения.
/// Здесь он пустой: всё, что нужно, провайдеры создают сами.
pub struct TradingApp;
pub struct BacktestApp;

delegate_components! {
    TradingApp {
        OrderIdTypeProviderComponent: UseType<RestOrderId>,
        ErrorTypeProviderComponent: UseType<HttpExchangeError>,
        OrderSubmitterComponent: SubmitViaRest,
    }
}

delegate_components! {
    BacktestApp {
        OrderIdTypeProviderComponent: UseType<SimOrderId>,
        ErrorTypeProviderComponent: UseType<Infallible>,
        OrderSubmitterComponent: SubmitToSimulator,
    }
}
```

Код, который зовёт `self.submit_order(order)`, при этом не меняется.
Никакого `dyn`, никакой рефлексии: всё разрешается на этапе компиляции и сворачивается в прямые вызовы, а неиспользованная реализация в бинарь не попадает.
Ассоциированные типы тоже становятся частью сборки: `HasErrorType` позволяет контексту выбрать свою ошибку, не трогая логику.

Стоит держать в голове три вещи. CGP работает на стабильном Rust, но требует свежий компилятор.
Синтаксис макросов заметно менялся от версии к версии — сниппеты выше писались под 0.7, и сверяться стоит с актуальной книгой, а не с постами двухлетней давности; заодно у макросов есть более «сахарная» форма записи (`#[cgp_impl]`, `#[uses]`), которую я здесь не показываю — она короче, но неявно переписывает `Self` и позиции генериков, а для первого знакомства это лишнее.
И главное — диагностика: ошибка в декларации `delegate_components!` (не тот провайдер, пропущенный компонент) приходит десятками сообщений о сгенерированных типах, поэтому у проекта есть отдельный `cargo-cgp`, который переписывает их в человеческий вид.

Авторы сами очерчивают границу применимости: одна реализация — обычный трейт или функция; маленький закрытый набор вариантов — `enum` и `match`; выбор в рантайме (плагины, конфиг, гетерогенные коллекции) — `dyn Trait`.
CGP окупается там, где реализаций реально несколько и выбор принадлежит контексту. Из заметного продакшена — hermes-sdk, новая версия IBC-релеера Informal Systems, откуда и выросла парадигма.

## Итог части 2 и что дальше

В части 1 типы делали недопустимые состояния невыразимыми, здесь они описывают договорённости между компонентами.

Trait задаёт контракт: что требуется от реализации.
Associated types уточняют контракт до конкретных типов: вход выбирает вызывающий, выход — реализация.
Const generics параметризуют тип значением, известным на этапе компиляции.

Нужно грамотно выбирать, где обобщать, а где конкретный тип проще, так как абстракции увеличивают сложность кода:
bound-ы расползаются по сигнатурам, мономорфизация увеличивает размер бинарника и время сборки, ошибки в generic-коде читаются хуже.
Обобщать стоит, когда реализаций больше одной; пока реализация одна и вторая не предвидится, конкретный тип проще.

До сих пор каждая проверка была про один тип: этот клиент умеет `submit_order`, у этого стакана десять уровней.
В части 3 соберём из типов структуры посложнее — списки на уровне типов (HList), валидаторы, работающие до запуска программы, и event sourcing, где корректность цепочки событий проверяет компилятор.