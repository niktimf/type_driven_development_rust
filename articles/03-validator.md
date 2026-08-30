# Type-driven development в Rust. Часть 3/5: проверяем данные ещё до запуска — type-level lists (HList), compile-time validators, event sourcing

В части 2 заявка уходила на площадку через контракт `ExchangeClient`:
`DraftOrder::submit` принимал любого клиента, который его выполнил,
и возвращал идентификатор площадки.
Всё это происходило на стороне клиента.
В этой части мы на стороне биржи и принимаем ту же заявку.

Наша биржа — та, к которой часть 2 подключалась как к `RestExchange` и `FixExchange`.
У неё две точки входа: по REST приходит JSON, по FIX — сообщение из тегов.
Разбирает их слой приёма, и он же проверяет кратность цены тику и объёма лоту
своей спецификацией инструмента: это smart constructor из части 1, только на стороне биржи,
и повторять его здесь не будем.
Из JSON и из FIX-сообщения слой приёма собирает одну и ту же структуру:

```rust
pub struct IncomingOrder {
    client_id: ClientOrderId,
    side: Side,
    order_type: OrderType<Usd>,
    quantity: Quantity,
}
```

Валюта котировки у нашей биржи одна, поэтому `Usd` зашит в тип.
Тип заявки — `OrderType<Usd>` из части 2: рыночная, лимитная или стоп-лимитная.

Цена и объём в этой структуре уже проверены, но биржа ей всё равно не доверяет:
дальше начинаются проверки, которых у клиента нет.
Торгуется ли инструмент прямо сейчас, в коридоре ли цена вокруг последней сделки,
не превышает ли заявка лимит по номиналу и не выведет ли она позицию участника за лимит.

Проверок несколько, и состав их известен при компиляции: биржа не узнаёт в рантайме,
что ей понадобилось проверять позицию.
Но записан этот состав в коде россыпью, вызов за вызовом.

## Типы-списки (HList)

Список на уровне типов — это тип, в котором перечислены другие типы: не значения,
а сами типы, в определённом порядке.
Готовой конструкции для этого в Rust нет, но она собирается из двух структур,
как cons-список в Lisp, только из типов.
Такой список называют HList, heterogeneous list (гетерогенный список):
элементы в нём разных типов, ради этого он и нужен.

### Проблема: цепочка проверок вручную

Первая версия шлюза — четыре вызова подряд:

```rust
fn accept(order: IncomingOrder, ctx: &ExchangeCtx) -> Result<IncomingOrder, Rejection> {
    HaltCheck::check(&order, &ctx.status)?;
    PriceBandCheck::check(&order, &ctx.market)?;
    NotionalLimitCheck::check(&order, &ctx.limits)?;
    PositionLimitCheck::check(&order, &ctx.account)?;
    Ok(order)
}
```

Работает, пока набор проверок один на всех.
Но у биржи он не один: инструменту на аукционе коридор цены не нужен,
у маркет-мейкера с особым статусом другой лимит номинала,
в тестовом контуре проверку позиции выключают.
Каждый вариант набора — своя функция или флаги в `ExchangeCtx`,
и код шлюза расходится по копиям.

Вторая версия — сделать набор данными:

```rust
trait DynCheck {
    fn check(&self, order: &IncomingOrder, ctx: &ExchangeCtx) -> Result<(), Rejection>;
}

struct Gate {
    checks: Vec<Box<dyn DynCheck>>,
}
```

Набор теперь собирается из конфига, но у этой версии есть проблемы.
Сигнатура одна на всех: каждая проверка получает весь `ExchangeCtx`,
хотя проверке статуса нужен только статус, а проверке позиции — только счёт.
Состав списка виден только в рантайме: какие проверки стоят в шлюзе, из типа `Gate` не узнать.
И пропавшую проверку сборка не ловит: шлюз без проверки коридора собирается и работает,
пока цена не выйдет за коридор, и обнаруживается это уже при инциденте.

В части 2 похожая задача уже решалась: `SupportedDepth` был списком допустимых глубин,
собранным вручную, по одному `impl` на значение.
Здесь нужен список как тип, с которым можно работать в общем виде, не перечисляя элементы руками.

### Решение: список из типов

Список из типов собирается из двух структур:

```rust
pub struct HNil;
pub struct HCons<Head, Tail>(Head, Tail);

type Checks = HCons<HaltCheck, HCons<PriceBandCheck, HCons<NotionalLimitCheck, HNil>>>;
```

`HNil` — пустой список, `HCons` — элемент и хвост, хвост — снова список.
`Checks` — один тип, в котором записаны три проверки в определённом порядке.

Проверка — свой тип с собственным контекстом.
Контекст описан ассоциированным типом с параметром времени жизни — GAT из части 2:
проверка заимствует ровно тот кусок состояния биржи, который ей нужен.

```rust
trait Check {
    type Ctx<'a>;
    fn check(order: &IncomingOrder, ctx: Self::Ctx<'_>) -> Result<(), Rejection>;
}

struct HaltCheck;

impl Check for HaltCheck {
    type Ctx<'a> = &'a InstrumentStatus;

    fn check(_order: &IncomingOrder, status: &InstrumentStatus) -> Result<(), Rejection> {
        if status.halted { Err(Rejection::Halted) } else { Ok(()) }
    }
}
```

У `PriceBandCheck` контекст — состояние рынка, и тип заявки здесь уже имеет значение:

```rust
struct PriceBandCheck;

impl Check for PriceBandCheck {
    type Ctx<'a> = &'a MarketState;

    fn check(order: &IncomingOrder, market: &MarketState) -> Result<(), Rejection> {
        let price = match order.order_type {
            OrderType::Market => return Ok(()),   // своей цены нет
            OrderType::Limit(price) | OrderType::StopLimit { limit: price, .. } => price,
        };
        /* сравнение с коридором вокруг market.reference */
    }
}
```

У `NotionalLimitCheck` контекст — кортеж из двух ссылок, `(&Limits, &MarketState)`:
номинал рыночной заявки считается по цене последней сделки, и одних лимитов проверке мало.
У `PositionLimitCheck` — счёт участника.
Раздаёт проверкам их куски мастер-контекст, по одному `impl` на проверку:

```rust
trait Provide<C: Check> {
    fn provide(&self) -> C::Ctx<'_>;
}

impl Provide<HaltCheck> for ExchangeCtx {
    fn provide(&self) -> &InstrumentStatus { &self.status }
}
```

Осталось прогнать список.
Любая операция над HList — рекурсия по нему, и записывается она двумя `impl`-ами:
база на `HNil`, шаг на `HCons`:

```rust
trait RunChecks<Ctx> {
    fn run(order: &IncomingOrder, ctx: &Ctx) -> Result<(), Rejection>;
}

impl<Ctx> RunChecks<Ctx> for HNil {
    fn run(_order: &IncomingOrder, _ctx: &Ctx) -> Result<(), Rejection> { Ok(()) }
}

impl<C, Tail, Ctx> RunChecks<Ctx> for HCons<C, Tail>
where
    C: Check,
    Ctx: Provide<C>,
    Tail: RunChecks<Ctx>,
{
    fn run(order: &IncomingOrder, ctx: &Ctx) -> Result<(), Rejection> {
        C::check(order, ctx.provide())?;
        Tail::run(order, ctx)
    }
}
```

Шаг рекурсии читается так: голова списка — проверка `C`,
контекст обязан уметь выдать ей её кусок, хвост обязан уметь прогоняться дальше.
Компилятор разворачивает `Checks::run` в те же четыре вызова, что были в первой версии,
только теперь их порядок и состав записаны в типе, а не в теле функции.

Что получаем:
- Состав цепочки — тип.
Шлюз для аукциона и шлюз для непрерывных торгов — разные типы с разными списками,
а код прогона у них общий.
- Каждая проверка видит только свой контекст. Проверке статуса не достаётся счёт участника,
и в сигнатуре это видно.
- Новая проверка — новый тип и один `impl Provide`. Прогон не меняется.
- Цепочка останавливается на первом отказе: `?` в шаге рекурсии.

### Хорошие практики

**Рекурсия по списку — это всегда пара `impl`-ов.**
Один на `HNil`, один на `HCons`;
так устроена любая операция над HList — прогон, поиск, подсчёт длины.
Если операция не раскладывается на базу и шаг, HList под неё не подходит.

**Список — не для однородного.**
Если элементы одного типа с одной сигнатурой, хватит массива или `Vec`, и `dyn` тоже не нужен.
HList нужен там, где элементы разнотипны по-настоящему, здесь — по контексту:
у проверок разные требования к состоянию биржи.

**Глубина списка видна в ошибках.**
Ошибка на элементе в глубине приходит с полным типом списка:
десять проверок — десять уровней `HCons` в сообщении компилятора.
Псевдоним `type Checks = ...` сокращает объявление, но не сообщения об ошибках.

### В библиотеках

- `frunk` — HList в готовом виде: те же `HCons`/`HNil`,
макрос `hlist!` для сборки и трейт `Generic`,
который превращает обычную структуру в HList и обратно — одна операция над списком применяется
к любой структуре с подходящими полями. Всё на стабильном Rust, без `unsafe`.
- `typenum` — числа как типы: `U0`..`U1024` и арифметика над ними трейтами.
Так до const generics был устроен `generic-array`, о котором шла речь в части 2.
Число здесь тоже список, только из битов: `UInt<UInt<UTerm, B1>, B0>` читается как двоичное `10`,
то есть `U2`.

Наш `RunChecks` принимает любой список проверок, в том числе без `PriceBandCheck`:
обязательный состав цепочки нигде не записан.

## Compile-time валидатор

Compile-time валидатор — это проверка, которую выполняет компилятор при сборке типа,
а не программа при обработке данных.
Его предмет — сама цепочка: из каких проверок она состоит и куда можно передать заявку,
прошедшую их.

### Проблема: список собрали, но полный ли он

Шлюз, параметризованный списком проверок, собирается с любым списком:

```rust
pub struct Gate<Checks> {
    _checks: PhantomData<Checks>,
}

let gate: Gate<HCons<HaltCheck, HCons<NotionalLimitCheck, HNil>>> = Gate { _checks: PhantomData };
```

Проверки коридора в этом списке нет, и ничто на это не указывает: шлюз собирается, заявки проходят,
цена уходит от последней сделки на сколько угодно.
Обязательные проверки есть — в голове у автора, в документации, в тестах, где угодно, но не в типе.

### Решение: предикат `Contains`

Нужно утверждение о списке: «список содержит проверку `C`».
На уровне типов утверждение — это трейт, а его доказательство — `impl`.
Записывается оно рекурсией по списку, как и прогон,
только теперь у рекурсии есть свидетель — индекс, на котором элемент нашёлся:

```rust
pub struct Here;
pub struct There<Index>(PhantomData<Index>);

pub trait Contains<C, Index> {}

impl<C, Tail> Contains<C, Here> for HCons<C, Tail> {}

impl<C, Head, Tail, Index> Contains<C, There<Index>> for HCons<Head, Tail>
where
    Tail: Contains<C, Index>,
{}
```

Первый `impl`: если искомый тип — голова списка, индекс `Here`.
Второй: если хвост содержит искомый тип на индексе `Index`,
весь список содержит его на индексе `There<Index>`.
Для `HNil` реализации нет: поиск, дошедший до конца списка, проваливается.
Индекс выводит компилятор: для `PriceBandCheck` в списке из трёх проверок это `There<Here>`,
и писать его руками не нужно.

Шлюз теперь требует обязательный минимум в конструкторе:

```rust
impl<Checks> Gate<Checks> {
    pub fn new<I1, I2>() -> Self
    where
        Checks: Contains<HaltCheck, I1> + Contains<PriceBandCheck, I2>,
    {
        Gate { _checks: PhantomData }
    }
}

let gate: Gate<HCons<HaltCheck, HCons<NotionalLimitCheck, HNil>>> = Gate::new();
// error[E0277]: the trait bound `HNil: Contains<PriceBandCheck, _>` is not satisfied
```

Индексы `I1` и `I2` — параметры конструктора, а не типа `Gate`: компилятор выводит их на вызове,
и в `Gate<Checks>` они не попадают.
Поле `_checks` приватное, других способов собрать шлюз нет.

### Проблема: шлюз можно обойти

Список полный, но прогон по нему пока возвращает `Result<(), Rejection>`:
сама заявка после прогона остаётся той же `IncomingOrder`, что и до него.
Функция постановки в стакан принимает `IncomingOrder`, и ничто не мешает вызвать её в обход шлюза.

### Решение: прогон меняет тип

Пусть прогон возвращает другой тип:

```rust
pub struct Valid<Checks> {
    order: IncomingOrder,
    _checks: PhantomData<Checks>,
}

impl<Checks> Gate<Checks> {
    pub fn accept(
        &self,
        order: IncomingOrder,
        ctx: &ExchangeCtx
    ) -> Result<Valid<Checks>, Rejection>
    where
        Checks: RunChecks<ExchangeCtx>,
    {
        Checks::run(&order, ctx)?;
        Ok(Valid { order, _checks: PhantomData })
    }
}
```

`Valid<Checks>` — заявка вместе с доказательством, что она прошла цепочку `Checks`.
Поля приватные, и выдаёт `Valid` только шлюз: это smart constructor из части 1,
только доказывает он не свойство значения, а факт прогона.

Потребители требуют каждый своего: постановке в стакан нужна проверка номинала,
маржинальному расчёту — проверка позиции:

```rust
pub fn accept_into_book<Checks, I>(
    valid: Valid<Checks>,
    id: OrderId
) -> Order<Working>
where
    Checks: Contains<NotionalLimitCheck, I>,
{ /* матчинг — за кадром */ }

pub fn reserve_margin<Checks, I>(
    valid: &Valid<Checks>,
    market: &MarketState,
) -> Money<Usd>
where
    Checks: Contains<PositionLimitCheck, I>,
{ /* ... */ }
```

Сам матчинг в статье не показываем: граница проходит по сигнатуре.
В `accept_into_book` нельзя передать `IncomingOrder` — только `Valid`, а `Valid` выдаёт только шлюз.
И пропуск от шлюза, в списке которого нет `PositionLimitCheck`,
в `reserve_margin` не примут — ошибка та же, E0277 про `HNil`.

Порядок проверок типом не фиксируем.
От него зависит только скорость отказа: дешёвую проверку статуса выгодно ставить раньше
дорогой проверки позиции, но результат прогона от перестановки не меняется.
Такое решают при объявлении списка, bound под него не нужен.

Забытую проверку компилятор называет прямо: дошёл до `HNil` и не нашёл `PriceBandCheck`:

```
error[E0277]: the trait bound `HNil: Contains<PriceBandCheck, _>` is not satisfied
```

Дубль проверки в списке он называет хуже: `Contains<HaltCheck, _>` теперь доказывается
двумя способами, и компилятор просит аннотацию типа, хотя проблема в списке:

```
error[E0283]: type annotations needed
   = note: multiple `impls` satisfying `HCons<HaltCheck, HCons<HaltCheck, ...>>: Contains<HaltCheck, _>` found
```

Что получаем:
- Обязательный состав цепочки записан в сигнатуре `Gate::new`, и шлюз без него не собрать.
- Проверенная заявка — отдельный тип. В стакан и в маржинальный расчёт принимают только его.
- У каждого потребителя свой минимум: в стакан — с номиналом, в маржу — с позицией.
Bound растёт там, где проверка нужна, а не у всех сразу.

### Хорошие практики

**Индексы-свидетели — в функциях, не в типах.**
Параметр `I` в `fn new<I>()` выводится на вызове и исчезает.
Параметр `I` в `struct Gate<Checks, I>` пришлось бы писать руками в каждой сигнатуре,
где встречается `Gate`.

**Доказательство — `PhantomData` плюс приватное поле.**
Собрать `Valid` в обход шлюза нельзя по той же причине,
по которой в части 1 нельзя было собрать `Price` в обход `InstrumentSpec`.
Без приватного поля доказательства нет:
`Valid { order, _checks: PhantomData }` соберёт кто угодно.

**Требуйте минимум.**
Шлюз требует две проверки, потребитель — одну свою.
Если `Gate::new` потребует все четыре, шлюз для аукциона без коридора цены не соберётся вовсе,
хотя он там и не нужен.

### В библиотеках

- `typed-builder` — тот же предикат полноты, но для полей структуры:
builder несёт в generic-параметрах, какие поля уже заданы,
и `.build()` без обязательного поля не компилируется. Пропущенное поле — ошибка сборки,
а не паника в рантайме.
- `static_assertions` — проверки при компиляции макросами: `const_assert!` для констант,
`assert_impl_all!` для трейтов,
`assert_fields!` для полей. Там, где утверждение о типе не выражается bound-ом,
его можно записать так.

Заявка прошла шлюз и встала в стакан.
Дальше с ней что-то происходит — исполнение, отмена — и биржа обязана помнить,
что именно и в каком порядке.

## Event sourcing

Event sourcing — способ хранить не состояние объекта, а историю событий, которые к нему привели.
Текущее состояние вычисляется из истории, а не хранится отдельно.

### Проблема: состояние — производное от истории

Заявка в стакане исполняется по частям, отменяется, истекает.
Регулятор спрашивает, что именно с ней произошло и в каком порядке;
разбор инцидента начинается с того же вопроса.
Поле `status` на это не отвечает: в нём текущее положение заявки, а не путь к нему.

Хранить надо события, и тип для них уже есть: `OrderEvent` из части 1.
Там он тоже был на стороне биржи, но служил примером вложенного `enum` и уходил в лог;
здесь он становится записью журнала, и требований к нему больше.
Берём его с тремя изменениями.
У каждого события появился `id`: журнал общий на все заявки, и событие обязано знать, чьё оно.
В `Accepted` добавился `quantity`: по журналу заявку нужно восстановить целиком, а в части 1
событие только логировалось.
Цена стала `Price<Usd>` — в `Filled` и внутри `OrderType` у `Accepted`: валюта дошла и до событий.

```rust
pub enum OrderEvent {
    Accepted { order_id: OrderId, side: Side, order_type: OrderType<Usd>, quantity: Quantity },
    Filled { order_id: OrderId, price: Price<Usd>, quantity: Quantity },
    Cancelled { order_id: OrderId, reason: CancelReason },
}
```

`enum` описывает форму событий, но не их порядок.
`Filled` после `Cancelled` — такая же бессмыслица, как рыночная заявка с лимитной ценой из части 1,
и записать её в журнал ничто не мешает.

### Решение: переход как `impl`

Состояние заявки — в типе, как в typestate из части 1; события стороны записи — отдельные типы:

```rust
pub mod state {
    pub struct Working;
    pub struct Filled;
    pub struct Cancelled;
}

pub struct Order<State> { /* id, side, order_type, quantity */ }

pub struct Fill { price: Price<Usd>, quantity: Quantity }
pub struct Cancel { reason: CancelReason }
```

Переход — трейт, параметризованный событием.
Он меняет тип заявки и одновременно порождает запись для журнала:

```rust
pub trait Apply<E> {
    type Next;
    fn apply(self, event: E) -> (Self::Next, OrderEvent);
}

impl Apply<Fill> for Order<state::Working> {
    type Next = Order<state::Filled>;

    fn apply(self, fill: Fill) -> (Order<state::Filled>, OrderEvent) {
        let event = OrderEvent::Filled { id: self.id, price: fill.price, quantity: fill.quantity };
        (self.transition(), event)
    }
}

impl Apply<Cancel> for Order<state::Working> {
    type Next = Order<state::Cancelled>;
    /* ... */
}
```

`impl`-ов ровно два, оба для `Working`.
Для `Filled` и `Cancelled` реализаций нет, и переходов из них для компилятора не существует:

```rust
let (cancelled, _) = working.apply(Cancel { reason: CancelReason::ByUser });
cancelled.apply(fill);
// error[E0599]: no method named `apply` found for struct `Order<Cancelled>`
```

В части 1 typestate запрещал вызывать `fill` на отменённой заявке тем,
что метода нет в `impl`-блоке.
Здесь запрет тот же, только переход теперь возвращает ещё и событие,
и записать в журнал `Filled` после `Cancelled` наш код не может.
`Fill` берётся из матчинга, который за кадром; в примерах его подставляет заглушка.

Что получаем:
- Допустимые переходы перечислены `impl`-ами. Новый переход — новый `impl`, недопустимый —
его отсутствие.
- Событие появляется вместе с переходом. Записать событие, не сменив состояние,
или сменить состояние, не записав событие, нельзя:
у `apply` один выход на оба.
- Терминальное состояние — тип без `impl Apply`. Флаг `is_closed` не нужен.

### Что типы здесь не проверяют

Реплей — восстановление состояния из сохранённых событий — типами покрыт не целиком.
События приходят из хранилища данными, `enum`,
и какое состояние у заявки сейчас, известно только в рантайме.
Держать его приходится тоже в `enum`, но варианты несут типизированные состояния, а не метки.
Шаг реплея — метод `step`: взять текущее состояние и одно событие из журнала,
найти пару в `match` и позвать её `apply`:

```rust
pub enum OrderState {
    Working(Order<state::Working>),
    Filled(Order<state::Filled>),
    Cancelled(Order<state::Cancelled>),
}

pub enum ReplayError { NotAccepted, IllegalTransition }

impl OrderState {
    /// Один шаг реплея: по событию из журнала зовём `apply` текущего состояния.
    fn step(self, event: &OrderEvent) -> Result<Self, ReplayError> {
        match (self, event) {
            (Self::Working(order), OrderEvent::Filled { price, quantity, .. }) => {
                let (filled, _already_journaled) = order.apply(Fill { price: *price, quantity: *quantity });
                Ok(Self::Filled(filled))
            }
            (Self::Working(order), OrderEvent::Cancelled { reason, .. }) => {
                let (cancelled, _already_journaled) = order.apply(Cancel { reason: *reason });
                Ok(Self::Cancelled(cancelled))
            }
            // Без `_`: новый вариант `OrderState` или `OrderEvent` ломает этот `match`,
            // пока ветки не дописаны, — E0004, как у `match` в части 1.
            (Self::Working(_), OrderEvent::Accepted { .. })
            | (Self::Filled(_), _)
            | (Self::Cancelled(_), _) => Err(ReplayError::IllegalTransition),
        }
    }
}

pub fn replay(events: &[OrderEvent]) -> Result<OrderState, ReplayError> {
    let (first, rest) = events.split_first().ok_or(ReplayError::NotAccepted)?;
    rest.iter().try_fold(OrderState::try_from(first)?, OrderState::step)
}
```

Сам реплей — две строки из std: `split_first` отделяет первое событие,
`try_fold` сворачивает остальные с ранним выходом по ошибке.
`OrderState::try_from(first)` — `impl TryFrom<&OrderEvent>`: история начинается только с `Accepted`,
из него собирается `Order<Working>`, любое другое первое событие — `NotAccepted`.
Правила переходов не переписаны: внутри веток `step` те же `apply`, что и на стороне записи.
Ветку `(Cancelled(order), Filled) => order.apply(Fill { .. })` в этот `match` не добавить:
у `Order<Cancelled>` нет ни одного `impl Apply`, и компилятор не найдёт метод.
`step` только выбирает, какой из разрешённых переходов звать,
и возвращает `IllegalTransition`, если для пары «состояние и событие» нет `impl`.
Событие, которое `apply` возвращает вторым, уже в журнале — в `step` оно не нужно.

Типы гарантируют, что наш код не запишет невозможный переход и не выполнит его при реплее.
Журнал, пришедший извне — из другой версии сервиса, после ручной правки, с повреждённого диска, —
всё равно проверяется в рантайме: пара «состояние и событие», для которой нет `impl`,
становится `ReplayError`, и сигнатура это показывает.

### Хорошие практики

**`match` в реплее — без `_`.**
Автомат записан дважды: `impl`-ами `Apply` и ветками `step`,
и свести их в одно место без макросов не получится.
Компилятор сверяет их с двух сторон: недопустимую ветку в `step` не написать, потому что нет `impl`,
а новый вариант `OrderState` или `OrderEvent` ломает `match`, пока ветки не дописаны, —
тот же E0004, что у `match` в части 1.
Для допустимой пары можно вернуть `Err` вместо `apply`, и этого компилятор не заметит;
заметит тест: реплей журнала со стороны записи обязан вернуть ту же заявку.

**Событие несёт всё, что нужно для восстановления.**
Состояние вычисляется из журнала, поэтому поле, которого нет ни в одном событии,
при реплее останется пустым.
`quantity` в `Accepted` появился именно поэтому: без него `replay` собрал бы заявку без объёма.
Проверка простая: `replay` по журналу должен возвращать ту же заявку, что была на стороне записи.

**Терминальное состояние — тип без `impl Apply`.**
У `Order<Cancelled>` нет методов перехода, и этого достаточно.
Проверка «а не закрыта ли заявка» в начале каждого метода не нужна.

### В библиотеках

- `cqrs-es` — фреймворк CQRS и event sourcing: трейт `Aggregate` с ассоциированными `Command`,
`Event`,
`Error`. Граница между записью и чтением у него та же: `handle` возвращает `Result`,
`apply(&mut self, event)` — нет. Реплей доверяет уже записанным событиям и вернуть ошибку не может;
наш `replay` её возвращает.
