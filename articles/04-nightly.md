# Type-driven development в Rust. Часть 4/5: выходим за стабильный Rust — generic const exprs, const traits, gen blocks, pattern types

«Жизнь есть только на nightly» в сообществе Rust говорят давно и обычно как шутку,
но в каждой шутке есть доля правды:
самые свежие и интересные фичи компилятора появляются именно там.
Для системы типов это не шутка: её фичи ждут стабилизации дольше всего.
`!` в позиции типа шёл к stable больше десяти лет, стабилизацию для 1.41 в 2019-м откатили,
не дожидаясь релиза, а specialization с 2016 года так и не вышла.
Кто выражает инварианты в типах, рано или поздно пишет `#![feature]` в первой строке крейта,
и эта часть о том, что в ней сейчас.

В частях 1–3 мы моделировали заявки, подключения к площадкам и биржевой шлюз на stable.
В части 2 размер пакета заявок проверялся в `const`-блоке внутри функции.
Сборка отклоняла пакет больше лимита площадки, но `cargo check` его пропускал.
Контракт фида использовал GAT, чтобы отдавать уровни стакана по ссылке.
Оба решения работают на стабильном Rust.
В этой части посмотрим, какие возможности nightly позволяют изменить эти примеры
и какие ограничения при этом остаются.

Всё проверено на nightly от 2026-08-31 (rustc 1.100.0-nightly).
Версия закреплена в `rust-toolchain.toml`.
Диагностика и ограничения ниже относятся к этому компилятору.

У const traits и pattern types менялся синтаксис.
При обновлении nightly примеры под `#![feature]` нужно проверять заново.

## `generic_const_exprs`

В части 2 const-параметры задавали глубину `OrderBook<DEPTH>` и длину массива `[DraftOrder; N]`.
На stable параметр `N` можно использовать как длину массива,
но выражение `[T; N + 1]` недоступно.
Фича `generic_const_exprs` разрешает выражения над generic-параметрами
в позиции const-аргумента.
Через такой аргумент можно выразить и условие `N <= MAX` в bound-е.

### Проблема: лимит пакета проверяется после мономорфизации

У площадки из части 2 есть лимит на размер пакета, ассоциированная константа `Exchange::MAX_BATCH`.
Пакет там записан массивом с длиной в типе, и проверить `N <= MAX_BATCH` хотелось бы при компиляции.
Булево условие нельзя записать непосредственно в `where`.
В части 2 для проверки использовался `const`-блок:

```rust
pub fn submit_batch_typed<EC, const N: usize>(
    exchange_client: &EC,
    orders: [DraftOrder<EC::Quote>; N],
) -> [Result<EC::ExchangeOrderId, EC::Error>; N]
where
    EC: ExchangeClient + Exchange,
{
    const { assert!(N <= EC::MAX_BATCH, "batch exceeds exchange limit") };
    orders.each_ref().map(|order| exchange_client.submit_order(order))
}
```

Такая проверка срабатывает после мономорфизации,
когда `const`-блок вычисляется при инстанцировании функции под конкретные `EC` и `N`:

```
$ cargo check      # проходит
$ cargo build
error[E0080]: evaluation panicked: batch exceeds exchange limit
   = note: while instantiating `submit_batch_typed::<RestExchange, 10>`
```

В этом примере `cargo check` проходит, а полная сборка отклоняет конкретный вызов.

### Решение: условие в bound-е

Условие можно выразить через тип с параметром `bool`.
Реализация трейта будет только у варианта с `true`:

```rust
#![feature(generic_const_exprs)]
#![allow(incomplete_features)]

pub struct Assert<const COND: bool>;

pub trait IsTrue {}
impl IsTrue for Assert<true> {}

pub fn submit_batch<EC, const N: usize>(
    exchange_client: &EC,
    orders: [DraftOrder<EC::Quote>; N],
) -> [Result<EC::ExchangeOrderId, EC::Error>; N]
where
    EC: ExchangeClient + Exchange,
    Assert<{ N <= EC::MAX_BATCH }>: IsTrue,
{
    orders.each_ref().map(|order| exchange_client.submit_order(order))
}
```

Фича позволяет подставить `{ N <= EC::MAX_BATCH }` в const-параметр `Assert`.
На вызове с конкретными `EC` и `N` компилятор вычисляет выражение и проверяет bound:
`Assert<true>` реализует `IsTrue`, а `Assert<false>` не реализует.

Пакет в лимите проходит, а пакет больше лимита не компилируется,
и ошибка теперь приходит на `cargo check`, по месту вызова:

```rust
let rest = RestExchange::default();          // MAX_BATCH у RestExchange — 5
let six: [DraftOrder<Usd>; 6] = /* шесть черновиков */;
submit_batch(&rest, six);
```

```
$ cargo check
error[E0308]: mismatched types
   |
   |     submit_batch(&rest, six);
   |     ^^^^^^^^^^^^^^^^^^^^^^^^ expected `false`, found `true`
   |
   = note: expected constant `false`
              found constant `true`
note: required by a bound in `submit_batch`
```

Сообщение называет значения, а не условие: `Assert<{ N <= EC::MAX_BATCH }>` вычислился
в `Assert<false>`, а единственная реализация `IsTrue` написана для `Assert<true>`.
Где именно лежит условие, видно по строке `required by a bound`, она указывает на `where`.

Проверка размера теперь входит в контракт функции и срабатывает уже на `cargo check`.
Значение лимита по-прежнему задаётся в `Exchange::MAX_BATCH`.
Как и в части 2, тело отправляет заявки по одной:
проверка длины массива сама по себе не делает отправку пакетной или атомарной.

### Хорошие практики

**Фичу включает и вызывающий крейт.**
Bound с `{ N <= EC::MAX_BATCH }` компилятор вычисляет на стороне вызова,
и без `#![feature(generic_const_exprs)]` в крейте вызывающего он этого сделать не может.
Отвергается даже корректный вызов, и ошибка указывает внутрь чужого кода:

```
error[E0284]: type annotations needed
  --> tdd-04-nightly/src/const_bounds.rs:59:1
   |
59 | / pub fn submit_batch<EC, const N: usize>(
```

В части 2 `const`-блок стоял в теле функции: деталь реализации,
и от подключающего крейта ничего не требовалось.
При переносе проверки в bound зависимость от фичи появляется в публичной сигнатуре.
Крейту, который вызывает эту функцию, тоже нужен nightly с включённой фичей.

**Один `Assert` на одно условие.**
`Assert<{ .. }>: IsTrue` в ошибке печатается как `expected false, found true`,
условие в тексте не названо.
Если условий несколько, по одним значениям `true` и `false` их не различить.
Держите условие в отдельной строке `where` и сразу над ним комментарий,
на который читатель попадёт по `required by a bound`.

**Предупреждение `incomplete_features` стоит прочитать.**
Без `#![allow(incomplete_features)]` компилятор сообщает,
что фича неполна и может привести к его падению.
Для внутреннего API можно согласованно обновлять компилятор и вызывающий код.
В публичной библиотеке такая сигнатура навязывает пользователям экспериментальную фичу.

### Статус фичи

- `#![feature(generic_const_exprs)]`,
[tracking issue #76560](https://github.com/rust-lang/rust/issues/76560),
помечена `incomplete_features`.
- На nightly от 2026-08-31 не работает с новым trait solver: компилятор пишет
`feature(generic_const_exprs) is not supported with the next-generation trait solver`
и сам откатывает крейт на старый solver.
- Для стабилизации разрабатывается отдельное подмножество,
[`min_generic_const_args`](https://doc.rust-lang.org/unstable-book/language-features/min-generic-const-args.html).
Оно допускает const-аргументы в виде путей, без произвольных выражений над параметрами.
На nightly от 2026-08-31 это ещё заготовка: и путь `Batch<E::MAX_BATCH>` (E0747),
и выражение `{ N <= E::MAX_BATCH }` под ней отклоняются.
Сравнение из этого раздела в подмножество не входит;
для показанного bound-а на выбранном компиляторе нужен `generic_const_exprs`.

## Const traits

`const fn` на стабильном Rust может вызывать другую `const fn`, но не метод трейта.
Const traits снимают это ограничение: трейт объявляется `const`,
реализация тоже, и тогда её методы работают в `const`-контексте.

### Проблема: сравнение лимитов недоступно в `const`

В части 3 лимиты по номиналу заявки и позиции проверялись в рантайме.
Теперь на стороне торгового клиента зададим лимиты потерь: на сделку и на торговый день,
а также дневной порог предупреждения.
Их рабочие значения приходят из конфига.
В коде зададим максимально допустимые значения:
проверка конфига отклоняет превышение, а изменение самих максимумов требует новой сборки.
Пока запишем их отдельными константами:

```rust
const MAX_LOSS_PER_TRADE: Money<Usd> = usd(dec!(50_000.00));
const WARN_DAILY_LOSS: Money<Usd> = usd(dec!(750_000.00));
const MAX_DAILY_LOSS: Money<Usd> = usd(dec!(1_000_000.00));
```

Для этого раздела уточним `Money` из части 2:
все суммы одной валюты будут храниться с одинаковым числом знаков после запятой.
Это правило представления выбранной модели.
Например, `750_000.0` и `750_000.00` обозначают одну сумму,
но конструктор примет только запись с масштабом `Currency::MINOR_UNITS`.
Проверка масштаба работает при сборке уже на stable:
`Money::new` объявлен как `const fn` и возвращает `Result`.
`usd` разбирает результат `match`-ем и на `Err` паникует,
а паника в `const`-контексте — это ошибка сборки:

```rust
pub struct Money<C> {
    amount: Decimal,
    _currency: PhantomData<C>,
}

impl<C: Currency> Money<C> {
    pub const fn new(amount: Decimal) -> Result<Self, MoneyError> {
        if amount.scale() != C::MINOR_UNITS { return Err(MoneyError::Scale); }
        Ok(Self { amount, _currency: PhantomData })
    }
}

const fn usd(amount: Decimal) -> Money<Usd> {
    match Money::new(amount) {
        Ok(money) => money,
        Err(e) => panic!("{}", e.message()),
    }
}
```

`MINOR_UNITS` — число знаков после запятой из `Currency` части 2, у доллара их два.
Литерал `750_000.0` с одним знаком не собирается, а текст ошибки берётся из `MoneyError`:

```
error[E0080]: evaluation panicked: масштаб не совпадает с валютой
   |
   | const WARN_DAILY_LOSS: Money<Usd> = usd(dec!(750_000.0));
   |                                     ^^^^^^^^^^^^^^^^^^^^ evaluation of `WARN_DAILY_LOSS` failed inside this call
```

Лимит из конфига в рантайме проходит через тот же `Money::new` и получает `Err`, а не панику.

Сравнение этих сумм через `<` на stable в `const`-контексте недоступно.
Порог предупреждения должен быть ниже лимита остановки торгов.
Оператор `<` на `Money` вызывает `PartialOrd::lt`,
а обычную реализацию этого трейта нельзя вызвать при вычислении константы.
На выбранном nightly попытка использовать её выглядит так:

```rust
impl<C> PartialOrd for Money<C> { /* сравнение мантисс */ }

const _: () = assert!(WARN_DAILY_LOSS < MAX_DAILY_LOSS, "предупреждение должно срабатывать раньше остановки");
```

```
error[E0277]: the trait bound `Money<Usd>: const PartialOrd` is not satisfied
   |
   | const _: () = assert!(WARN_DAILY_LOSS < MAX_DAILY_LOSS, "...");
   |                       ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
   |
help: make the `impl` of trait `PartialOrd` `const`
   |
   | const impl<C> PartialOrd for Money<C> {
   | +++++
```

Диагностика требует константную реализацию `PartialOrd`.
На stable можно написать отдельную `const fn` для сравнения мантисс;
nightly позволяет использовать стандартный оператор `<`.

### Решение: `const trait`, `const impl`, `[const]`

Слово `const` появляется в трёх местах: в объявлении трейта, в реализации и в bound-е.

В объявлении трейта `const` означает, что реализации разрешено быть константной.
В `std` так уже объявлены `PartialEq`, `PartialOrd`, операторы из `core::ops`, `Clone`, `Default`:

```rust
pub const trait PartialOrd<Rhs = Self>: [const] PartialEq<Rhs> {
    fn partial_cmp(&self, other: &Rhs) -> Option<Ordering>;
    // ...
}
```

В реализации `const` означает, что её можно вызывать в `const`-контексте.
Конструктор проверил одинаковый масштаб всех `Money<C>` одной валюты,
поэтому достаточно сравнить мантиссы.
Собственное сравнение `Decimal` в используемой версии библиотеки не объявлено `const`:

```rust
#![feature(const_trait_impl, const_cmp, const_ops)]

const impl<C> PartialEq for Money<C> {
    fn eq(&self, other: &Self) -> bool { self.amount.mantissa() == other.amount.mantissa() }
}

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
```

В bound-е `[const]` означает «в `const`-контексте нужна константная реализация,
в рантайме подойдёт любая».
Это позволяет написать общую проверку возрастающей последовательности.
При сборке она сравнит константы, в рантайме проверит значения из конфига:

```rust
const fn ascending<T: [const] PartialOrd>(steps: &[T]) -> bool {
    let mut i = 1;
    while i < steps.len() {
        if !(steps[i - 1] < steps[i]) { return false; }
        i += 1;
    }
    true
}
```

Теперь `assert!` с оператором `<` собирается.
Для примера зададим дневной максимум как двадцать максимумов потерь на сделку.
Это выбранное правило конфигурации, а не расчёт дневного риска:
оно не учитывает число сделок, открытые позиции или проскальзывание.
Для этого контура также потребуем,
чтобы максимум на сделку был ниже дневного порога предупреждения.
Умножение на целое — это `Mul::mul`, ещё один метод трейта, и `const impl` для него пишется так же.
Масштаб оно не меняет: мантисса умножается, `scale` остаётся, и сравнение мантисс остаётся верным:

```rust
const impl<C> Mul<u32> for Money<C> {
    type Output = Money<C>;
    fn mul(self, n: u32) -> Money<C> {
        // from_i128_with_scale у Decimal не const: мантисса собирается через from_parts
        let mantissa = match self.amount.mantissa().checked_mul(n as i128) {
            Some(mantissa) => mantissa,
            None => panic!("переполнение мантиссы i128"),
        };
        let amount = from_mantissa(mantissa, self.amount.scale());
        Money { amount, _currency: PhantomData }
    }
}

const fn from_mantissa(mantissa: i128, scale: u32) -> Decimal {
    let (negative, m) = (mantissa < 0, mantissa.unsigned_abs());
    assert!(m <= (1_u128 << 96) - 1, "мантисса не помещается в Decimal");
    Decimal::from_parts(m as u32, (m >> 32) as u32, (m >> 64) as u32, negative, scale)
}

const MAX_LOSS_PER_TRADE: Money<Usd> = usd(dec!(50_000.00));
const MAX_DAILY_LOSS: Money<Usd> = MAX_LOSS_PER_TRADE * 20;
const WARN_DAILY_LOSS: Money<Usd> = usd(dec!(750_000.00));

const _: () = assert!(WARN_DAILY_LOSS < MAX_DAILY_LOSS, "предупреждение должно срабатывать раньше остановки");
const _: () = assert!(
    ascending(&[MAX_LOSS_PER_TRADE, WARN_DAILY_LOSS, MAX_DAILY_LOSS]),
    "ступени лимитов должны расти: сделка < предупреждение < остановка"
);
```

Поменяйте множитель на `10`: дневной потолок станет `500_000.00`, ниже порога предупреждения,
и сборка остановится с текстом из `assert!`:

```
error[E0080]: evaluation panicked: предупреждение должно срабатывать раньше остановки
   |
   | const _: () = assert!(WARN_DAILY_LOSS < MAX_DAILY_LOSS, "...");
   |               ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ evaluation of `_` failed here
```

Константы сравниваются и вычисляются через стандартные операторы на `Money`.
Эти же реализации работают в рантайме.
Компилятор проверяет заданные соотношения между константами;
соблюдение лимитов во время торгов по-прежнему проверяет риск-модуль.

### Хорошие практики

**`[const] Trait` в bound-е, `const trait` в объявлении.**
Первое читается «константная реализация нужна только в `const`-контексте»,
второе разрешает реализациям быть константными вообще.
Без `[const]` тело `ascending` не собирается, хотя `T: PartialOrd` на месте:
компилятор отвечает `the trait bound T: [const] PartialOrd is not satisfied` на строке с `<`.

**Константной становится не вся библиотека.**
Из `const`-методов `Decimal` здесь нужны `mantissa()`, `scale()` и `from_parts`.
Сравнение и арифметика `Decimal` в этой версии не константные,
поэтому в примере они реализованы через мантиссы.
Сравнение мантисс верно только потому, что `Money::new` уравнял масштаб по `MINOR_UNITS`,
а поле приватное, и обойти конструктор нельзя, как у `Price` в части 1.
Сам по себе `const_trait_impl` не делает обычные методы зависимости константными.
При сборке мантиссы через `from_parts` нужно проверить,
что она помещается в 96 бит: приведения к `u32` иначе отбросят старшие биты.
В примере обе проверки завершаются паникой, то есть ошибкой сборки в `const`-контексте.
Для сумм из внешнего ввода нужен метод с `Result`,
чтобы превышение диапазона не приводило к панике во время работы.

**Фич нужно три.**
`const_trait_impl` даёт синтаксис,
`const_cmp` разрешает `const impl` для `PartialEq` и `PartialOrd`,
`const_ops` — для `Add`, `Mul` и остальных операторов.
Без двух последних `const impl` для трейта из `std` не собирается даже для своего типа:
`E0658: use of unstable const library feature const_cmp`, `trait is not stable as const yet`.

### Статус фичи

- `#![feature(const_trait_impl)]`,
[tracking issue #67792](https://github.com/rust-lang/rust/issues/67792).
Константность трейтов `std` вынесена в отдельные фичи:
`const_cmp` (rust-lang/rust#143800) и `const_ops` (rust-lang/rust#143802).
- Синтаксис менялся дважды за последние годы.
Форма `#[const_trait]` над трейтом, `impl const Trait for Type` и bound `~const Trait`
на nightly от 2026-08-31 уже не парсится: компилятор отвечает `expected a trait, found type`.
Актуальны `const trait`, `const impl` и `[const]`.
- В `std` фича применена широко: `PartialEq`, `PartialOrd`, `Clone`, `Default`, `Borrow`,
`Destruct` и операторы из `core::ops` объявлены как `const trait`,
и на них опираются `bool::then`, `Option::ok_or` и другие константные методы.
- Но не везде: `PartialOrd` у `Duration` — обычный derive,
и сравнение двух таймаутов в `const` пока не собирается:
`trait PartialOrd is implemented but not const`.

## Gen-блоки

`gen`-блок — это тело, в котором значения отдают через `yield`,
а на выходе получается обычный `impl Iterator`.
Состояние между вызовами `next` хранит сам блок: локальные переменные, позиция в цикле.

### Проблема: у итератора появляется состояние

Фид части 2 отдавал уровни стакана наружу без копирования,
и ради этого ассоциированный тип получил собственное время жизни:

```rust
pub trait MarketDataFeed {
    type Quote: Currency;

    type Levels<'a>: Iterator<Item = &'a Level<Self::Quote>>
    where
        Self: 'a;

    fn bids(&self) -> Self::Levels<'_>;
}

impl<Quote: Currency> MarketDataFeed for BookFeed<Quote> {
    type Quote = Quote;
    type Levels<'a> = std::slice::Iter<'a, Level<Quote>> where Self: 'a;

    fn bids(&self) -> Self::Levels<'_> { self.bids.iter() }
}
```

Здесь nightly уже не нужен: `impl Trait` в возвращаемом типе метода трейта стабилен с версии 1.75,
и GAT из сигнатуры убирается на стабильном Rust.

```rust
pub trait MarketDataFeed {
    type Quote: Currency;

    fn bids(&self) -> impl Iterator<Item = &Level<Self::Quote>>;
}
```

Именованный тип `Levels<'a>` больше не нужен.
Заимствование осталось: время жизни ссылок в итераторе связано с `&self`.
GAT полезен, если вызывающему коду нужно обращаться к типу `Levels<'a>`
и ставить на него дополнительные bound-ы.
Для одного метода `bids` достаточно возвращаемого `impl Iterator`.

Другое дело, когда итератор нужно построить поверх фида.
Пусть стратегии нужен агрегированный стакан:
соседние уровни объединяются в ценовые интервалы, а их объёмы складываются.
Итератор хранит текущий интервал и выдаёт результат при переходе к следующему.
После окончания входа он должен выдать последний накопленный интервал.

На stable это можно записать через `from_fn` или комбинацию адаптеров.
Однако одного `map` недостаточно: он выдаёт по результату на каждый входной элемент.
`scan` хранит состояние, но возврат `None` из его замыкания означает конец итерации,
а после исчерпания входа замыкание не вызывается для выдачи остатка.
Для агрегации придётся отдельно выразить пропуск промежуточных результатов и обработку конца.
Другой вариант на stable — структура с собственной реализацией `Iterator`.

В этом примере используем упрощённый `Level`:
цена записана целым числом тиков, объём — целым числом лотов одного инструмента.
Это отличается от `Level<Quote>` из части 2 и позволяет сосредоточиться на итераторе.

```rust
#[derive(Clone, Copy)]
pub struct Level {
    pub price: u64,
    pub quantity: u64,
}
```

На вход подаём одну сторону стакана с уровнями, отсортированными по цене.
`bucket` задаёт ширину интервала в тиках и должен быть больше нуля.
Суммарный объём считаем помещающимся в `u64`.
Формула ниже округляет цену вниз до границы интервала;
это метка группы для анализа стакана, а не цена отправляемой заявки.

```rust
enum AggregationState {
    Empty,
    Accumulating(Level),
    Finished,
}

pub struct Aggregated<I: Iterator<Item = Level>> {
    inner: I,
    bucket: u64,
    state: AggregationState,
}

impl<I: Iterator<Item = Level>> Iterator for Aggregated<I> {
    type Item = Level;

    fn next(&mut self) -> Option<Level> {
        loop {
            match &mut self.state {
                AggregationState::Empty => match self.inner.next() {
                    Some(mut level) => {
                        level.price -= level.price % self.bucket;
                        self.state = AggregationState::Accumulating(level);
                    }
                    None => {
                        self.state = AggregationState::Finished;
                        return None;
                    }
                },
                AggregationState::Accumulating(acc) => match self.inner.next() {
                    Some(mut level) => {
                        level.price -= level.price % self.bucket;
                        if acc.price == level.price {
                            acc.quantity += level.quantity;
                        } else {
                            return Some(std::mem::replace(acc, level));
                        }
                    }
                    None => {
                        let last = *acc;
                        self.state = AggregationState::Finished;
                        return Some(last);
                    }
                },
                AggregationState::Finished => return None,
            }
        }
    }
}
```

Алгоритм проходит уровни один раз и накапливает объём текущего интервала.
Начальное состояние `Empty` означает, что первый уровень ещё не прочитан.
В `Accumulating` хранится текущий интервал.
При переходе к следующему интервалу `mem::replace` заменяет накопитель и возвращает предыдущий.
После конца входа итератор переходит в `Finished`, выдав остаток, если он есть.
Последующие вызовы `next` возвращают `None` без чтения входа.
В ручном `Iterator` каждый вызов `next` начинает выполнять метод заново.
Поэтому состояние хранится в поле `state`,
а выдача очередного интервала завершает вызов через `return`.

### Решение: цикл вместо структуры

В `gen`-блоке накопитель и цикл остаются локальными,
а следующий интервал выдаётся через `yield`:

```rust
#![feature(gen_blocks)]

fn aggregate(levels: impl Iterator<Item = Level>, bucket: u64) -> impl Iterator<Item = Level> {
    gen move {
        let mut acc: Option<Level> = None;
        for level in levels {
            let price = level.price - level.price % bucket;
            match acc {
                Some(ref mut a) if a.price == price => a.quantity += level.quantity,
                Some(a) => {
                    yield a;
                    acc = Some(Level { price, quantity: level.quantity });
                }
                None => acc = Some(Level { price, quantity: level.quantity }),
            }
        }
        if let Some(a) = acc {
            yield a;
        }
    }
}
```

Структуру состояния больше не нужно объявлять вручную.
Накопитель остаётся локальной переменной, а последний интервал выдаётся после цикла.
На выходе всё тот же `impl Iterator`, и на одном стакане оба варианта дают один результат:

```
manual    = [Level { price: 18550, quantity: 3 }, Level { price: 18545, quantity: 9 }, ...]
generated = [Level { price: 18550, quantity: 3 }, Level { price: 18545, quantity: 9 }, ...]
```

Машину состояний для такого цикла создаёт компилятор.
Вызывающий код по-прежнему получает `impl Iterator`.
Блок можно использовать и в методе трейта,
возвращающем `impl Iterator<Item = Level>`.

### Хорошие практики

**Тело `gen`-блока выполняется при продвижении итератора.**
Создание итератора не запускает тело.
В тестовом стакане первый `next` читает два уровня:
второй уже относится к другому интервалу, поэтому первый можно выдать.
Побочные эффекты внутри `gen` случаются в момент чтения, а не в момент создания.

**`gen move` и заимствование.**
Без `move` способ захвата зависит от использования переменной.
В нашем примере `levels` потребляется циклом,
но `bucket` захватывается по ссылке, и вернуть такой итератор нельзя.
`gen move` задаёт захват обеих переменных по значению.
Если захвачено значение-ссылка, её время жизни по-прежнему ограничивает итератор.
У замыканий действуют те же правила захвата.

**`?` внутри блока не работает.**
Оператор требует, чтобы `Result` или `Option` возвращал сам блок,
а `gen`-блок возвращает `()`:

```
error[E0277]: the `?` operator can only be used in a gen block that returns
              `Result` or `Option` (or another type that implements `FromResidual`)
```

Ошибки из такого итератора отдают элементами: `yield Err(..)`,
а вызывающий собирает их через `collect::<Result<Vec<_>, _>>()`.

**Когда хватает `from_fn`.**
`std::iter::from_fn` подходит и для показанной агрегации:
замыкание может захватить несколько переменных и читать вход в цикле.
В каждом вызове оно возвращает следующий элемент или `None`.
`gen` позволяет записать цикл один раз и приостанавливать его через `yield`,
сохраняя локальные переменные между вызовами `next`.

### Статус фичи

- `#![feature(gen_blocks)]`,
[RFC 3513](https://github.com/rust-lang/rfcs/pull/3513) принят 07.04.2024.
- Слово `gen` зарезервировано редакцией 2024.
В редакции 2021 тот же блок не собирается: `gen` там обычный идентификатор,
и на слове `move` после него компилятор отвечает
`expected one of ... or an operator, found keyword move`.
- Кроме блоков под той же фичей работают `gen fn` и `async gen`:
первая описывает генератор функцией целиком,
вторая даёт `impl AsyncIterator` для асинхронных потоков.

## Pattern types

Pattern type — это базовый тип плюс паттерн, которому обязано соответствовать значение.
`usize is 1..` — все `usize`, кроме нуля: множество значений сужено предикатом,
а представление в памяти то же, что у `usize`.
Идея близка к refinement types, уточняющим типам,
но ограничение здесь задаётся паттерном, как в `match`.
На выбранном nightly pattern type проверяется как отдельный тип:
автоматического преобразования в базовый тип или из него нет.

В части 2 доступные глубины стакана перечислялись реализациями `SupportedDepth`.
Теперь попробуем выразить более простое правило: поле глубины не может содержать ноль.
Pattern type объявляется через макрос `std::pat::pattern_type!`:

```rust
#![feature(pattern_types, pattern_type_macro)]
use std::pat::pattern_type;

/// Ненулевая глубина как значение, без привязки к длине массива.
type Depth = pattern_type!(usize is 1..);

struct Snapshot { depth: Depth }
```

Литерал вне диапазона отклоняется при компиляции:

```rust
let s = Snapshot { depth: 10 };   // компилируется
let s = Snapshot { depth: 0 };
// error[E0308]: mismatched types
//    expected `pattern_type!(usize is 1..)`, found integer
```

Здесь запрещён ноль в поле `Snapshot::depth`.
Связи с длиной массива нет: `Snapshot` не заменяет `OrderBook<DEPTH>`
и не проверяет список поддерживаемых стратегией глубин.
На stable ненулевое поле можно описать через `NonZeroUsize`.

Нулевое значение pattern type невозможно, и компилятор использует его для кодирования `None`:
`Option<Depth>` занимает столько же, сколько сам `usize`, как и `Option<NonZeroUsize>`:

```
size_of Option<Depth> = 8, Option<usize> = 16, usize = 8
```

Эти размеры получены на 64-битной целевой платформе примеров.

В показанном безопасном коде значение задаётся литералом подходящего диапазона.
Глубина из конфига — это обычный `usize`, и присвоить его нельзя:

```rust
let n: usize = 10;
let s = Snapshot { depth: n };
// error[E0308]: expected `pattern_type!(usize is 1..)`, found `usize`
```

Обратно тоже: `s.depth` не присвоить в `usize` и не разобрать по `match`,
обе попытки дают ту же E0308.
В примерах крейта есть преобразование через `unsafe transmute` после проверки `n >= 1`.
Корректность проверки и совместимость представлений при этом обеспечивает программист.
Для глубины из конфига `NonZeroUsize::new(n)` уже даёт безопасный вариант на stable.

Паттерны с диапазоном поддерживают только целочисленные типы и `char`:

```rust
type Bad = pattern_type!(f64 is 1.0..);
// error[E0277]: `f64` is not a valid base type for range patterns
//    only integer types and `char` are supported
```

Ограничения `Price` на `Decimal` так не выразить:
они требуют проверки положительности и кратности шагу конкретного инструмента.
Smart constructor из части 1 по-прежнему нужен.
В нашем примере pattern types проверяют диапазон литерала при компиляции,
но безопасного преобразования обычного `usize` после проверки пока нет.

### Статус фичи

- `#![feature(pattern_types)]` и `#![feature(pattern_type_macro)]`,
[tracking issue #123646](https://github.com/rust-lang/rust/issues/123646).
- Первая фича помечена внутренней:
`the feature pattern_types is internal to the compiler or standard library`,
и компилятор добавляет `using it is strongly discouraged`.
- Формат предложения — это MCP в репозитории types-team, а не RFC.
Внутри компилятора и `core` фича уже работает: через неё описан `*const T is !null`,
и нулевой адрес там отдан под `None` в `Option<NonNull<T>>`.
- Синтаксис `u32 is 1..` без макроса, который часто встречается в постах,
на nightly от 2026-08-31 не парсится.

## Never type

Часть 1 объявляла `ClientOrderId` с `type Err = Infallible`, потому что `!`
в произвольных позициях типа был за feature-флагом.
В выбранный nightly уже вошла стабилизация для Rust 1.100:
feature-флаг не нужен, и тот же контракт можно записать напрямую.

```rust
impl FromStr for ClientOrderId {
    type Err = !;

    fn from_str(s: &str) -> Result<Self, !> {
        Ok(Self(s.to_string()))
    }
}
```

Разворачивание становится короче, чем в части 1.
Там пустая ветка требовала `match never {}`, здесь значение типа `!` подставляется само,
потому что приводится к любому типу:

```rust
let id: ClientOrderId = match "order-2026-0001".parse::<ClientOrderId>() {
    Ok(id) => id,
    Err(never) => never,
};
```

В симуляторе части 2 `type Error = Infallible`.
На этом компиляторе это уже псевдоним `!`, поэтому результат разбирается без `unwrap`:

```rust
let Ok(SimOrderId(n)) = sim.submit_order(&draft);
```

В размерах это видно по цифрам: `Option<!>` занимает ноль байтов,
а `Result<u64, !>` ровно столько же, сколько сам `u64`, потому что второй вариант невозможен.

В Rust 1.100 `Infallible` становится псевдонимом:

```rust
pub type Infallible = !;
```

Код частей 1 и 2 после этого продолжает работать без правок,
`Result<T, Infallible>` и `Result<T, !>` — это один тип,
а документация `std` советует писать `!` напрямую.

### Статус фичи

- Стабилизация включена 24.08.2026
([PR rust-lang/rust#155499](https://github.com/rust-lang/rust/pull/155499)).
В stable выходит с 1.100.
На stable 1.98 тот же код отвечает `error[E0658]: the ! type is experimental`.
- Имя `Infallible` сохранено в `std` как псевдоним.
Примеры частей 1 и 2 можно продолжать использовать без замены имени типа.
- Путь к стабилизации у этой фичи самый долгий из пяти:
`!` как тип расходящейся функции работал с версии 1.0,
а в произвольной позиции типа — только с 1.100.
Попыток стабилизации, по счёту автора влитого PR, было шесть;
ту, что для 1.41, откатили из-за never-type fallback.

## Итог части 4 и что дальше

В этой части проверка длины пакета стала bound-ом и сработала на `cargo check`.
Const traits позволили сравнивать и умножать денежные константы стандартными операторами.
`gen`-блок сохранил локальное состояние агрегации между вызовами `next`.
У pattern types пока нет безопасного преобразования глубины из конфига,
а `!` уже вошёл в выбранный nightly без feature-флага.

Польза этих возможностей различается.
Bound позволяет раньше обнаружить неверный размер пакета,
но требует включать экспериментальную фичу и в вызывающем крейте.
Для сравнения констант и агрегации существуют решения на stable;
nightly позволяет записать их через привычные операторы и цикл с `yield`.
Версию компилятора для таких примеров стоит закрепить по дате
и при обновлении запускать тесты, включая примеры, которые должны отклоняться компилятором.

В примере из части 1 можно получить `WorkingOrder` после `submit` и выйти из функции,
не вызвав ни `fill`, ни `cancel`.
Typestate проверяет допустимость переходов между состояниями,
но не требует переводить заявку в конечное состояние.
Удаление локальной структуры само по себе не отменяет заявку на бирже.
Так же обстоит дело с резервом маржи: система должна явно учесть его освобождение,
а факт владения Rust-значением не гарантирует внешнюю операцию.

В части 5 обсудим линейные типы, которые требуют потребить значение ровно один раз,
и границы такой гарантии при работе с биржей.
Также рассмотрим эффекты, variadic generics для списков вроде HList из части 3
и view types для явного описания частичных заимствований.
Это направления развития языка; их обсуждение не означает обещания конкретного релиза.
