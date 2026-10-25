# Type-driven development в Rust. Часть 5/5: будущее системы типов — substructural types, effects, variadic generics, view types

Пожалуй, самая интересная и самая нестабильная часть серии.
В предыдущих частях мы моделировали заявки на stable и пробовали возможности nightly.
Теперь посмотрим на будущие фичи Rust, у которых ещё меняются правила и синтаксис.
Некоторые уже можно попробовать в компиляторе,
другие пока существуют в виде предложений и экспериментов.

Будущих фич у Rust много.
В этой статье разберём только самые интересные для type-driven development:
те, которые расширяют возможности системы типов.

## Linear types

Линейный тип требует использовать значение ровно один раз: потребить его или передать дальше.
Владение в Rust описывается аффинными типами:
значение можно использовать не более одного раза, но можно и не использовать.

### Проблема: резерв можно оставить без обработки

В конце части 4 `WorkingOrder` можно было оставить без вызова `fill` или `cancel`.
С резервом маржи похожая задача: после резервирования средства нужно закрепить или освободить.

```rust
struct MarginReservation {
    // Данные резерва.
}

impl MarginReservation {
    fn commit(self) {
        // Закрепить резерв за заявкой.
    }

    fn release(self) {
        // Освободить резерв.
    }
}
```

Методы принимают `self`, как переходы typestate из части 1.
Учёт средств опущен; нас интересует, обязан ли владелец вызвать один из методов.

```rust
fn process_order(_reservation: MarginReservation) {
    // Ни commit, ни release не вызваны.
}
```

Этот код компилируется: при выходе Rust уничтожит объект без закрепления или освобождения средств.

### Решение: обязательная обработка значения

Обычное уничтожение вызывает [`Drop::drop`](https://doc.rust-lang.org/std/ops/trait.Drop.html),
если он реализован, и уничтожает поля; [`drop`](https://doc.rust-lang.org/std/mem/fn.drop.html) делает это сразу.
А [`mem::forget`](https://doc.rust-lang.org/std/mem/fn.forget.html) забирает владение и пропускает уничтожение.
Так можно пропустить и автоматическое освобождение, если бы оно было в `Drop`.

В [обсуждаемом дизайне](https://goals.rust-lang.org/2026/move-trait.html) эти разрешения разделены:

| Трейт | Что разрешает |
| --- | --- |
| `Forget` | Пропустить уничтожение через `mem::forget(reservation)` |
| `Destruct` | Уничтожить резерв через `drop(reservation)` или при выходе из функции |

Для обязательного вызова `commit` или `release` нужно запретить оба действия.
Запись `impl !Forget` приведена в цели проекта;
`!Destruct` здесь схематично обозначает требуемый запрет обычного уничтожения.

```rust
// Иллюстративный синтаксис; это не работающий пример на выбранном nightly.
impl !Forget for MarginReservation {}
impl !Destruct for MarginReservation {}
```

Первая версия `process_order` тогда отклонялась бы из-за необработанного резерва.
В обеих ветках нужно передать его завершающему методу:

```rust
enum OrderDecision {
    Accept,
    Reject,
}

fn process_order(reservation: MarginReservation, decision: OrderDecision) {
    match decision {
        OrderDecision::Accept => reservation.commit(),
        OrderDecision::Reject => reservation.release(),
    }
}
```

`commit` и `release` должны разобрать `self` и обработать данные.
С пустым телом `self` уничтожался бы в конце метода, что запрещено при `!Destruct`.

### Что можно сделать на stable

В `Drop` можно освобождать резерв автоматически, если такая отмена соответствует правилам системы.
Но дождаться асинхронного запроса на биржу в обычном `Drop` нельзя: async drop на stable пока нет.
Явного решения от владельца резерва компилятор при этом не требует.

`#[must_use]` у `MarginReservation` предупреждает, если полученный резерв сразу проигнорировать.
Если сохранить резерв в переменную, предупреждения уже не будет:
атрибут не требует вызвать `commit` или `release` до конца функции.

## Ordered types

Упорядоченные типы требуют использовать значения ровно один раз и в порядке их введения.
В Rust это связано с неподвижностью значения:
его адрес в памяти не меняется до уничтожения.

### Проблема: самоссылочные структуры

Самоссылочная структура хранит ссылку на собственные данные.
При перемещении структуры поля меняют адреса, а ссылка продолжает указывать на прежнюю память.
В async-коде так может выглядеть машина состояний, которую компилятор создаёт из `async fn`.

Возьмём асинхронный `ExchangeClient` из части 2:

```rust
trait ExchangeClient {
    async fn submit_order(&self, order: &DraftOrder) -> Result<OrderId, ExchangeError>;
}
```

Функция владеет заявкой и передаёт клиенту ссылку на неё:

```rust
async fn place_order(
    exchange_client: &impl ExchangeClient,
    draft: DraftOrder,
) -> Result<OrderId, ExchangeError> {
    let order = &draft;
    let exchange_id = exchange_client.submit_order(order).await?;
    println!("Биржа приняла заявку {}", order.client_order_id().0);
    Ok(exchange_id)
}
```

На `.await` выполнение может приостановиться, а ссылка `order` нужна и после ответа биржи.
Назовём созданный компилятором future `OrderSubmission`.
Во время ожидания он схематично хранит:

```text
OrderSubmission
  draft: DraftOrder
  order: ссылка на draft внутри этого же OrderSubmission
  pending: future вызова submit_order(order)
  state: ожидание ответа биржи
```

Перемещение всего `OrderSubmission` могло бы сделать ссылку `order` недействительной.

### Что обеспечивает `Pin` сейчас

Код работает на stable благодаря [`Pin`](https://doc.rust-lang.org/std/pin/index.html).
[`Future::poll`](https://doc.rust-lang.org/std/future/trait.Future.html) принимает закреплённую ссылку:

```rust
trait Future {
    type Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output>;
}
```

До начала выполнения future из `async fn` можно перемещать: внутренних ссылок ещё нет.
Для опроса его сначала закрепляют; при обычном `.await` это происходит автоматически.
Компилятор задаёт таким future отсутствие `Unpin`;
другие реализации `Future` могут быть `Unpin`.

Для значения без `Unpin` безопасный API `Pin` запрещает извлечение с перемещением.
Сам указатель можно передавать дальше: адрес значения не меняется.
`!Unpin` само по себе не запрещает перемещения до закрепления.

Это библиотечный контракт, который поддерживают безопасный API и его `unsafe`-реализация.
При ручной обёртке над future нужно получить `Pin<&mut Inner>` из `Pin<&mut Outer>`.
Такую проекцию поля без `Unpin` обычно реализуют через `unsafe` или генерирующие его макросы.

### Решение: неподвижность как свойство типа

Предлагаемый `Move` разрешает перемещение значения в памяти.
Тип с `!Move` сохранял бы адрес с момента создания, и это проверял бы компилятор.
В [варианте преобразования async-кода Yosh Wuyts](https://blog.yoshuawuyts.com/self-referential-types/)
компилятор сам создавал бы неподвижное состояние future:

```rust
// Схема будущего кода компилятора, не работающий пример на выбранном nightly.
impl !Move for OrderSubmission {}
```

Такое значение нельзя было бы перенести в другую переменную или извлечь через `mem::replace`.
Его нужно сразу создавать по окончательному адресу;
для этого обсуждается [in-place initialization](https://blog.yoshuawuyts.com/placing-functions/).

### Статус фич

Неподвижность и гарантии уничтожения входят в
[принятую цель Rust на 2026–2027 годы](https://goals.rust-lang.org/2026/move-trait.html).
Правила ещё обсуждаются; показанные ограничения линейности пока не проверяются на выбранном nightly.

В [предложении Niko Matsakis](https://smallcultfollowing.com/babysteps/blog/2025/10/21/move-destruct-leak/)
используется иерархия `Forget: Destruct: Move`.
Текущая цель не требует `Destruct: Move`: неподвижное значение тоже может нуждаться в уничтожении.
Изменение существующего `Future` в неё не входит.
Заменить `Pin` на `!Move` напрямую нельзя: нужны другие правила создания и выполнения future.

## Effects

Эффекты описывают, какое поведение разрешено вычислению.
Например, `async` разрешает приостанавливать выполнение на `.await`.
Компилятор проверяет это разрешение:
непосредственно в теле обычной `fn` использовать `.await` нельзя.
Вызов `async fn` возвращает future, выполнения которого можно дождаться через `.await`.
`const fn` можно вызвать и в рантайме, и при компиляции,
но её тело должно соблюдать ограничения `const`, например обходиться без сетевых запросов.

Обобщение по эффектам, effect generics, позволило бы писать общий код для sync и async.

### Проблема: общий алгоритм для sync и async

Клиент биржи из части 2 может отправлять заявку асинхронно, а симулятор возвращает результат сразу.
Если сохранять обе формы API, получаются два трейта:

```rust
trait SyncExchangeClient {
    fn submit_order(&self, order: &DraftOrder) -> Result<OrderId, ExchangeError>;
}

trait AsyncExchangeClient {
    async fn submit_order(&self, order: &DraftOrder) -> Result<OrderId, ExchangeError>;
}
```

Алгоритм последовательной отправки пакета приходится записать для каждого контракта:

```rust
fn submit_batch<EC: SyncExchangeClient>(
    exchange_client: &EC,
    orders: &[DraftOrder],
) -> Vec<Result<OrderId, ExchangeError>> {
    let mut results = Vec::with_capacity(orders.len());
    for order in orders {
        results.push(exchange_client.submit_order(order));
    }
    results
}

async fn submit_batch_async<EC: AsyncExchangeClient>(
    exchange_client: &EC,
    orders: &[DraftOrder],
) -> Vec<Result<OrderId, ExchangeError>> {
    let mut results = Vec::with_capacity(orders.len());
    for order in orders {
        results.push(exchange_client.submit_order(order).await);
    }
    results
}
```

На stable можно сделать симулятор асинхронным и оставить одну `submit_batch_async`.
Обобщение нужно, если библиотека сохраняет и обычный синхронный вызов, и вызов с `.await`.

### Решение: обобщить код по эффекту выполнения

В [предложении Yosh Wuyts](https://blog.yoshuawuyts.com/extending-rusts-effect-system/)
трейт и вызывающий код могут быть обобщены по `async` через `#[maybe(async)]`:

```rust
// Иллюстрация предлагаемого дизайна, не работающий пример на выбранном nightly.
#[maybe(async)]
trait ExchangeClient {
    #[maybe(async)]
    fn submit_order(&self, order: &DraftOrder) -> Result<OrderId, ExchangeError>;
}

#[maybe(async)]
fn submit_batch<EC>(
    exchange_client: &EC,
    orders: &[DraftOrder],
) -> Vec<Result<OrderId, ExchangeError>>
where
    EC: #[maybe(async)] ExchangeClient,
{
    let mut results = Vec::with_capacity(orders.len());
    for order in orders {
        results.push(exchange_client.submit_order(order).await);
    }
    results
}
```

Эффект функции и bound-а должны согласовываться.
Для синхронного клиента компилятор создавал бы вариант без ожидания,
для асинхронного сохранял бы `.await`; режим выбирается при компиляции.
В части 4 похожую задачу для `const` решала `ascending<T: [const] PartialOrd>`:
bound требовал константную реализацию только в `const`-контексте.

### Границы обобщения

Если `release` у `MarginReservation` асинхронный, его future могут уничтожить до завершения.
Обобщение по `async` само по себе не гарантирует освобождения резерва.
Гарантии завершения и уничтожение ресурсов рассматриваются в
[Just add async](https://goals.rust-lang.org/2026/roadmap-just-add-async.html).

В [видении развития Rust](https://blog.yoshuawuyts.com/a-grand-vision-for-rust/)
обсуждаются также детерминизм и запрет обращений к API хоста, полезные для бэктеста.
Для этих ограничений нужен отдельный дизайн.

### Статус фичи

[Effects initiative](https://github.com/rust-lang/effects-initiative) развивает обобщение по `async` и `const`.
Окончательный синтаксис, включая запись `#[maybe(async)]`, ещё обсуждается.

## View types

View type — представление типа, в котором доступны только выбранные поля.
Ссылка на него ограничивает заимствование, позволяя независимо использовать остальные поля.

### Проблема: метод заимствует всю структуру

Соберём в одной структуре журнал событий из части 3 и книгу активных заявок:

```rust
struct ExchangeState {
    book: MatchingBook,
    journal: Vec<OrderEvent>,
    applied_events: usize,
}
```

Начнём с пустой книги и нулевого счётчика обработанных событий:

```rust
impl ExchangeState {
    fn rebuild_book(&mut self) {
        for event in &self.journal {
            self.book.apply_event(event);
            self.applied_events += 1;
        }
    }
}
```

Код компилируется: borrow checker различает заимствования отдельных полей.
Но вынесем обновление книги в метод:

```rust
impl ExchangeState {
    fn apply_to_book(&mut self, event: &OrderEvent) {
        self.book.apply_event(event);
        self.applied_events += 1;
    }

    fn rebuild_book(&mut self) {
        for event in &self.journal {
            self.apply_to_book(event);
        }
    }
}
```

Теперь возникает конфликт:

```text
error[E0502]: cannot borrow `*self` as mutable because it is also borrowed as immutable
```

Цикл сохраняет заимствование `journal`, а метод принимает `&mut self` на весь `ExchangeState`.
Вызывающий код проверяется по сигнатуре, даже если тело не использует журнал.
На stable можно передать нужные поля отдельными аргументами:

```rust
fn apply_to_book(
    book: &mut MatchingBook,
    applied_events: &mut usize,
    event: &OrderEvent,
) {
    book.apply_event(event);
    *applied_events += 1;
}

impl ExchangeState {
    fn rebuild_book(&mut self) {
        for event in &self.journal {
            apply_to_book(&mut self.book, &mut self.applied_events, event);
        }
    }
}
```

Можно также выделить книгу и счётчик в отдельный компонент с собственным методом.

### Решение: указать доступные поля в типе ссылки

В [обсуждаемом дизайне](https://smallcultfollowing.com/babysteps/blog/2026/03/22/max-min-view-types-followup/)
метод объявляет используемые поля `self`:

```rust
// Вариант предлагаемого синтаксиса, не работающий пример на выбранном nightly.
impl ExchangeState {
    fn apply_to_book(&mut self.{ book, applied_events }, event: &OrderEvent) {
        self.book.apply_event(event);
        self.applied_events += 1;
    }

    fn rebuild_book(&mut self) {
        for event in &self.journal {
            self.apply_to_book(event);
        }
    }
}
```

Тип такой ссылки можно записать как `&mut ExchangeState.{ book, applied_events }`.
`journal` через неё недоступен, поэтому его заимствование не конфликтует с вызовом метода.

[Эксперимент с view types](https://goals.rust-lang.org/2026/view-types-experiment.html)
принят как цель Rust на 2026 год: планируется сравнить несколько вариантов синтаксиса.

## Variadic generics

Variadic generics — обобщение по набору типов, количество которых заранее не фиксировано.
В конкретном использовании состав и длина набора известны при компиляции.

### Проблема: прогон проверок для кортежей разной длины

В части 3 цепочка проверок записывалась через HList:

```rust
type Checks = HCons<HaltCheck,
    HCons<PriceBandCheck,
    HCons<NotionalLimitCheck,
    HCons<PositionLimitCheck, HNil>>>>;
```

Две реализации `RunChecks`, для `HNil` и `HCons<C, Tail>`, работали со списком любой длины.
Тот же набор можно записать плоским кортежем:

```rust
type TupleChecks = (HaltCheck, PriceBandCheck, NotionalLimitCheck, PositionLimitCheck);
type SandboxTupleChecks = (HaltCheck, PriceBandCheck, NotionalLimitCheck);
```

Но у такого кортежа нельзя обобщённо выделить первый тип и хвост, как у `HCons`.
Приходится перечислять элементы в реализации трейта:

```rust
impl<C1, C2, Ctx> RunChecks<Ctx> for (C1, C2)
where
    C1: Check,
    C2: Check,
    Ctx: Provide<C1> + Provide<C2>,
{
    fn run(order: &IncomingOrder, ctx: &Ctx) -> Result<(), Rejection> {
        C1::check(order, <Ctx as Provide<C1>>::provide(ctx))?;
        C2::check(order, <Ctx as Provide<C2>>::provide(ctx))?;
        Ok(())
    }
}
```

`<Ctx as Provide<C1>>::provide(ctx)` явно выбирает контекст первой проверки.
Другим длинам кортежа нужны собственные реализации.

### Что можно сделать на stable

HList остаётся рабочим решением, а для плоских кортежей реализации можно сгенерировать макросом:

```rust
macro_rules! impl_run_checks {
    ($($C:ident),+ $(,)?) => {
        impl<Ctx, $($C),+> RunChecks<Ctx> for ($($C,)+)
        where
            $(
                $C: Check,
                Ctx: Provide<$C>,
            )+
        {
            fn run(order: &IncomingOrder, ctx: &Ctx) -> Result<(), Rejection> {
                $(
                    $C::check(order, <Ctx as Provide<$C>>::provide(ctx))?;
                )+
                Ok(())
            }
        }
    };
}

impl_run_checks!(C1);
impl_run_checks!(C1, C2);
impl_run_checks!(C1, C2, C3);
impl_run_checks!(C1, C2, C3, C4);
```

После раскрытия это четыре отдельных `impl` для длин от одного до четырёх.
Для пятой проверки понадобится ещё один вызов макроса.

### Решение: обобщить реализацию по набору типов

В [одном из вариантов дизайна lang-team](https://github.com/rust-lang/lang-team/blob/main/src/design_notes/variadic_generics.md)
предлагается разбирать кортеж на первый тип и хвост.
`Tuple` обозначает трейт кортежей, а `(C, ...Tail)` — первый элемент `C` и остаток `Tail`:

```rust
// Иллюстрация одного из вариантов дизайна, не работающий пример на выбранном nightly.
impl<Ctx> RunChecks<Ctx> for () {
    fn run(_order: &IncomingOrder, _ctx: &Ctx) -> Result<(), Rejection> {
        Ok(())
    }
}

impl<C, Tail, Ctx> RunChecks<Ctx> for (C, ...Tail)
where
    C: Check,
    Ctx: Provide<C>,
    Tail: Tuple + RunChecks<Ctx>,
{
    fn run(order: &IncomingOrder, ctx: &Ctx) -> Result<(), Rejection> {
        C::check(order, <Ctx as Provide<C>>::provide(ctx))?;
        Tail::run(order, ctx)
    }
}
```

У `SandboxTupleChecks` хвост после `HaltCheck` был бы `(PriceBandCheck, NotionalLimitCheck)`.
Рекурсия завершалась бы на `()`, а первый отказ по-прежнему останавливал бы проверки.
Это позволило бы использовать плоские кортежи без отдельного `impl` для каждой длины.
Требования к составу цепочки, которые в части 3 выражал `Contains`, нужно описывать отдельно.
Сам прогон не требует обязательных проверок и не запрещает дубли.

### Статус фичи

Окончательный дизайн ещё не выбран.
Для подготовки RFC команда [собирала сценарии использования](https://blog.rust-lang.org/inside-rust/2025/09/22/variadic-generics-micro-survey/).
Показанные `Tuple` и `...Tail` относятся к одному из предложений.

## Compile-time reflection

Рефлексия позволяет получать сведения о структуре типа: его полях и вариантах `enum`.
Compile-time reflection даёт доступ к ним при компиляции.
Сегодня библиотеки часто получают код обхода полей через процедурные `derive`-макросы.
Рефлексия могла бы заменить часть таких макросов:
библиотека обходила бы поля обычным Rust-кодом без `derive` на каждом пользовательском типе.
Так можно было бы описать сериализацию `OrderEvent` или логирование произвольных структур.

[Принятая цель на 2026–2028 годы](https://goals.rust-lang.org/2026/reflection-and-comptime.html)
предусматривает эксперимент с рефлексией через `const fn`, вызываемые только при компиляции.
Произвольное создание новых типов в него не входит; ограничения видимости должны сохраняться.
Формат сериализации и совместимость версий журнала по-прежнему определяет библиотека.

## Function contracts

Контракт функции задаёт предусловие вызова и постусловие для результата.
Smart constructors из части 1 проверяли отдельное значение.
При резервировании `quantity` лотов нужно также сравнить его с доступным объёмом `available`:
тип вроде `NonZeroU64` выражает ненулевой объём, но не отношение двух независимых аргументов.

На nightly для этого есть экспериментальные
[атрибуты `requires` и `ensures`](https://doc.rust-lang.org/nightly/unstable-book/language-features/contracts.html):

```rust
#![feature(contracts)]
use core::contracts::{ensures, requires};

#[requires(quantity > 0 && quantity <= available)]
#[ensures(move |remaining| *remaining == available - quantity)]
fn reserve(available: u64, quantity: u64) -> u64 {
    available - quantity
}
```

Замыкание в `ensures` получает ссылку на результат и связывает его с исходными аргументами.
Проверки выполняются в рантайме с
[`-Z contract-checks=yes`](https://doc.rust-lang.org/nightly/unstable-book/compiler-flags/contract-checks.html);
по умолчанию они выключены.
На nightly-2026-08-31 `reserve(10, 3)` возвращает `7`,
а `reserve(10, 0)` с включёнными проверками приводит к `failed requires check` и abort.
Контракт не возвращает `Result` и не доказывает условия при компиляции.
Он проверяет договорённости между внутренними компонентами;
ожидаемые отказы для входных данных по-прежнему нужно обрабатывать обычным кодом.

## Итог части 5 и всей серии

Хорошая модель домена устроена так, чтобы в коде было трудно нарушить её правила.
Если модель строится вокруг примитивов и независимых полей,
типы обычно позволяют больше состояний, чем допускает предметная область.
Ограничения приходится проверять в коде или описывать в документации.
Правило «этот метод не вызывай после cancel» приходится соблюдать вручную.

В type-driven подходе мы выражаем такие ограничения через типы.
Они описывают допустимые состояния и операции над ними,
поэтому интерфейс функции позволяет передать только подходящее состояние.
Типов становится больше, а код получается многословнее.
Правила предметной области по-прежнему нужно подробно описывать,
но часть условий, которые раньше проверялись в рантайме, теперь проверяет компилятор.
Попытка передать функции неподходящее состояние приводит к ошибке сборки.
