# Type-driven development в Rust. Часть 4/5: выходим за стабильный Rust — generic const exprs, const traits, gen blocks, pattern types

Три части подряд гарантии строились одним способом: значение заворачивается в тип,
и всё, что о нём известно, записано в обёртке.
`Price` из части 1 это `Decimal` за приватным полем, `OrderBook<DEPTH>` из части 2 держит глубину
в параметре типа, `Valid<Checks>` из части 3 это заявка плюс `PhantomData`.
Там, где выразительности не хватало, части 1–3 обходились стабильным Rust:
`SupportedDepth` вместо `DEPTH >= 1`, `const`-блок вместо `N <= MAX_BATCH` в bound-е,
GAT и ручной итератор там, где хотелось просто цикла.

В этой части те же обходы смотрим с другой стороны: что с ними делает nightly.
Каждой фиче отведён свой раздел, и на входе в нём код из частей 1–3.
Разделы разной длины: у pattern types и never type материала меньше,
потому что первая фича сегодня работает на узком классе значений, а вторая уже стабилизирована.

Всё проверено на nightly от 2026-08-31, это rustc 1.100.0-nightly.
Крейт примеров пинует эту дату в `rust-toolchain.toml`, и все «компилятор говорит» ниже
относятся к ней.
У трёх фич из пяти синтаксис менялся за последний год,
так что код под `#![feature]` придётся править вслед за компилятором.

## `generic_const_exprs`

Const-параметр в части 2 был значением, известным при компиляции: `OrderBook<DEPTH>`,
`[DraftOrder; N]`.
Но выражения над ним стабильный Rust не разрешает: ни `[T; N + 1]`, ни `N <= MAX` в bound-е.
`generic_const_exprs` разрешает и то и другое: внутри `{ .. }` в позиции const-аргумента
можно использовать generic-параметры функции.

### Проблема: лимит пакета проверяется после мономорфизации

У площадки из части 2 есть лимит на размер пакета, ассоциированная константа `Exchange::MAX_BATCH`.
Пакет там записан массивом с длиной в типе, и проверить `N <= MAX_BATCH` хотелось при компиляции.
Записать это в `where` нельзя: `where` принимает только bound-ы.
Часть 2 обошла это `const`-блоком:

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

Такая проверка срабатывает позже, чем нужно.
`const`-блок вычисляется при инстанцировании функции под конкретные `EC` и `N`,
то есть после мономорфизации:

```
$ cargo check      # проходит
$ cargo build
error[E0080]: evaluation panicked: batch exceeds exchange limit
   = note: while instantiating `submit_batch_typed::<RestExchange, 10>`
```

`cargo check` промах не видит, ошибка приходит не на вызов, а на инстанцирование,
и если функция лежит в библиотеке, первым её увидит тот, кто библиотеку подключил.

### Решение: условие в bound-е

Раз `where` принимает только bound-ы, условие надо сделать bound-ом.
Для этого нужен тип-носитель, у которого реализация трейта есть только для `true`:

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

`{ N <= EC::MAX_BATCH }` это константное выражение над generic-параметрами,
и разрешает его именно `generic_const_exprs`.
Компилятор вычисляет его на месте вызова, получает `true` или `false`,
и дальше это обычная проверка bound-а: у `Assert<true>` реализация `IsTrue` есть,
у `Assert<false>` нет.

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

Что получаем:
- Ошибка на `cargo check` и по месту вызова, как у любого другого bound-а.
- Лимит берётся у площадки, из ассоциированной константы части 2, и в сигнатуре не дублируется.
- Та же фича даёт арифметику в типах: `[T; N + 1]`, `[T; N * 2]` в сигнатурах и полях.

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

Часть 2 прятала `const`-блок от публичного API, потому что промах находил пользователь библиотеки.
Bound на выражении спрятать не выйдет: он стоит в сигнатуре,
и фича из детали реализации становится требованием ко всем, кто библиотеку подключает.

**Один носитель на одно условие.**
`Assert<{ .. }>: IsTrue` в ошибке звучит как `expected false, found true`,
условие в тексте не названо.
Если условий в одной сигнатуре несколько, из сообщения не понять, какое сработало.
Держите условие в отдельной строке `where` и сразу над ним комментарий,
на который читатель попадёт по `required by a bound`.

**Предупреждение `incomplete_features` стоит прочитать.**
Без `#![allow(incomplete_features)]` компилятор сообщает,
что фича неполна и может привести к его падению.
Bound-ы на выражениях стоит держать на границе, которую контролируете сами,
и не выносить в API, который подключают другие.

### Статус фичи

- `#![feature(generic_const_exprs)]`, tracking issue rust-lang/rust#76560,
помечена `incomplete_features`.
- На nightly от 2026-08-31 не работает с новым trait solver: компилятор пишет
`feature(generic_const_exprs) is not supported with the next-generation trait solver`
и сам откатывает крейт на старый solver.
- Стабилизация в текущем виде не планируется.
Вместо неё в компиляторе растят уменьшенное подмножество, `min_generic_const_args`:
там const-аргументом будет путь вроде `EC::MAX_BATCH`, а не выражение над параметрами.
На nightly от 2026-08-31 это ещё заготовка: и путь `Batch<E::MAX_BATCH>` (E0747),
и выражение `{ N <= E::MAX_BATCH }` под ней отклоняются.
Сравнение из этого раздела в подмножество не входит по замыслу,
так что bound на выражении останется под `generic_const_exprs`.

## Const traits

`const fn` на стабильном Rust умеет звать другую `const fn`, но не метод трейта.
Const traits снимают это ограничение: трейт объявляется `const`,
реализация тоже, и тогда её методы работают в `const`-контексте.

### Проблема: таблица инструментов проверяется в рантайме

Смарт-конструктор части 1 проверял цену по спецификации инструмента:
положительна и кратна шагу цены.
Значения, которые он принимает, приходят не только из сети.
Референсные цены, границы коридора, лимиты по умолчанию для инструментов,
которые система знает заранее, лежат в коде литералами.

У фьючерса на индекс шаг цены `0.25`, и цена `185.30` ему не кратна.
Пока конструктор возвращает `Result` и зовётся в рантайме, такой литерал доживёт до запуска.

Проверка кратности на `Decimal` записывается и сейчас: `mantissa()` и `scale()` уже `const fn`.
Упирается всё в то, что нужно вокруг неё.
Сравнение двух цен, клонирование спецификации, значение по умолчанию — это методы трейтов,
а `const fn` не вызывает их даже под bound-ом:

```rust
const fn same<T: PartialEq>(a: &T, b: &T) -> bool { a == b }
```

```
error[E0277]: the trait bound `T: [const] PartialEq` is not satisfied
   |
   | const fn same<T: PartialEq>(a: &T, b: &T) -> bool { a == b }
   |                                                     ^^^^^^
   |
   = note: required for `&T` to implement `[const] PartialEq<&T>`
```

Компилятор называет недостающий bound по имени: `[const] PartialEq`.

### Решение: `const trait`, `const impl`, `[const]`

Слово `const` появляется в трёх местах, и значение у него в каждом своё.

В объявлении трейта `const` означает, что реализации разрешено быть константной.
В `std` так уже объявлены `Clone`, `Default`, `PartialEq`, `Borrow`:

```rust
pub const trait PartialEq<Rhs = Self> {
    fn eq(&self, other: &Rhs) -> bool;
    // ...
}
```

В реализации `const` означает, что эта конкретная реализация годится для `const`-контекста.
Для своих типов её пишут руками или получают из `derive_const`:

```rust
#![feature(const_trait_impl, derive_const, const_cmp)]

#[derive(Debug, Clone, Copy)]
#[derive_const(PartialEq)]
pub enum Side { Buy, Sell }

#[derive(Debug, Clone, Copy)]
pub struct Price<Quote> { amount: Decimal, _quote: PhantomData<Quote> }

const impl<Quote> PartialEq for Price<Quote> {
    fn eq(&self, other: &Self) -> bool {
        self.amount.mantissa() == other.amount.mantissa()
            && self.amount.scale() == other.amount.scale()
    }
}
```

В bound-е `[const]` означает «в `const`-контексте нужна константная реализация,
в рантайме подойдёт любая».
Это и делает `same` вызываемой при сборке:

```rust
const fn same<T: [const] PartialEq>(a: &T, b: &T) -> bool { a == b }
```

Теперь смарт-конструктор части 1 переписывается константной функцией,
а проверки становятся частью сборки:

```rust
impl<Quote> InstrumentSpec<Quote> {
    pub const fn price(&self, value: Decimal) -> Price<Quote> {
        assert!(value.mantissa() > 0, "цена не положительна");
        assert!(value.scale() == self.tick.scale(), "разный масштаб");
        assert!(value.mantissa() % self.tick.mantissa() == 0, "цена не кратна тику");
        Price { amount: value, _quote: PhantomData }
    }
}

const ES: InstrumentSpec<Usd> = InstrumentSpec::new(dec!(0.25));
const REFERENCE: Price<Usd> = ES.price(dec!(185.50));

const _: () = assert!(same(&REFERENCE, &ES.price(dec!(185.50))));
const _: () = assert!(same(&Side::Buy, &Side::Buy) && !same(&Side::Buy, &Side::Sell));
```

Конструктор здесь паникует вместо `Result`: в `const`-контексте паника это ошибка сборки,
и текст из `assert!` попадает прямо в неё.
Цена не на шаге не доживает до запуска:

```rust
const REFERENCE: Price<Usd> = ES.price(dec!(185.30));
```

```
error[E0080]: evaluation panicked: цена не кратна тику
   |
   | const REFERENCE: Price<Usd> = ES.price(dec!(185.30));
   |                               ^^^^^^^^^^^^^^^^^^^^^^ evaluation of `REFERENCE` failed inside this call
   |
note: inside `InstrumentSpec::<Usd>::price`
   |
   |         assert!(value.mantissa() % self.tick.mantissa() == 0, "цена не кратна тику");
   |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ the failure occurred here
```

Что получаем:
- Литералы в коде проверены тем же правилом, что и данные из сети, но при сборке.
- Реализация одна на два контекста: `const impl PartialEq` работает и в `const`, и в рантайме.
- Сообщение из `assert!` становится текстом ошибки компиляции,
и `note: inside` указывает, какая именно проверка не прошла.

### Хорошие практики

**`[const] Trait` в bound-е, `const trait` в объявлении.**
Первое читается «константная реализация нужна только в `const`-контексте»,
второе разрешает реализациям быть константными вообще.
Свой трейт объявляется так же, как `PartialEq` в `std`:

```rust
pub const trait Notional {
    fn notional(&self) -> Decimal;
}
```

**Обычная реализация под `[const]`-bound не подходит.**
Если убрать `const` из `impl`, вызов при сборке перестаёт компилироваться,
и компилятор подсказывает, что делать:

```
error[E0277]: the trait bound `Price<Usd>: const PartialEq` is not satisfied
   |
help: make the `impl` of trait `PartialEq` `const`
   |
   | const impl<Quote> PartialEq for Price<Quote> {
```

**Константной становится не вся библиотека.**
`Decimal` даёт в `const` только `mantissa()`, `scale()` и конструкторы, арифметики там нет.
Отсюда строгое равенство масштабов в проверке выше: приведение `185.5` к двум знакам
потребовало бы умножения, которого в `const` нет, и конструктор требует совпадения масштабов.
Это ограничение библиотеки, а не фичи.

**Фич нужно три.**
`const_trait_impl` даёт синтаксис, `derive_const` даёт `#[derive_const(...)]`,
а `const_cmp` открывает константные реализации сравнения из `std`.
Без последней тот же код падает с `E0658: use of unstable const library feature const_cmp`.

### Статус фичи

- `#![feature(const_trait_impl)]`, tracking issue rust-lang/rust#67792.
- Синтаксис менялся дважды за последние годы.
Форма `#[const_trait]` над трейтом, `impl const Trait for Type` и bound `~const Trait`
на nightly от 2026-08-31 уже не парсится: компилятор отвечает `expected a trait, found type`.
Актуальны `const trait`, `const impl` и `[const]`.
- В `std` фича применена широко: `Clone`, `Default`, `PartialEq`, `Borrow`, `Destruct`
объявлены как `const trait`, и на них опираются `bool::then`, `Option::ok_or` и другие
константные методы.

## Gen-блоки

`gen`-блок это тело, в котором значения отдают через `yield`,
а на выходе получается обычный `impl Iterator`.
Состояние между вызовами `next` хранит сам блок: локальные переменные, позиция в цикле.

### Проблема: у итератора появляется состояние

Фид части 2 отдавал уровни стакана наружу без копирования,
и ради этого ассоциированный тип получил собственное время жизни:

```rust
pub trait MarketDataFeed {
    type Levels<'a>: Iterator<Item = &'a Level<Self::Quote>>
    where
        Self: 'a;

    fn bids(&self) -> Self::Levels<'_>;
}

impl<Quote: Currency> MarketDataFeed for BookFeed<Quote> {
    type Levels<'a> = std::slice::Iter<'a, Level<Quote>> where Self: 'a;

    fn bids(&self) -> Self::Levels<'_> { self.bids.iter() }
}
```

Здесь nightly уже не нужен: `impl Trait` в возвращаемом типе метода трейта стабилен с версии 1.75,
и GAT из этого места уходит на стабильном Rust.

```rust
pub trait MarketDataFeed {
    fn bids(&self) -> impl Iterator<Item = &Level<Self::Quote>>;
}
```

Ассоциированный тип исчез вместе с параметром времени жизни, и реализация осталась той же строкой.
Этот долг части 2 закрывает стабильный компилятор, а не фичи из этой части.

Другое дело, когда итератор нужно построить поверх фида.
Стратегии смотрят не на сырой стакан, а на агрегированный: уровни собирают в корзины по шагу цены,
объёмы внутри корзины складывают.
Такому итератору нужно состояние: накопленная корзина, которую отдают,
когда цена перешла границу, и которую надо отдать ещё раз в конце, когда уровни кончились.

Комбинаторы под это не подходят:
`map` и `scan` отдают ровно один элемент на входной, а здесь их меньше,
и последний появляется уже после того, как вход закончился.
Остаётся написать структуру и реализовать `Iterator` руками:

```rust
pub struct Aggregated<I: Iterator<Item = Level>> {
    inner: I,
    bucket: u64,
    pending: Option<Level>,
}

impl<I: Iterator<Item = Level>> Iterator for Aggregated<I> {
    type Item = Level;

    fn next(&mut self) -> Option<Level> {
        loop {
            match self.inner.next() {
                Some(level) => {
                    let price = level.price - level.price % self.bucket;
                    match self.pending {
                        Some(ref mut acc) if acc.price == price => acc.quantity += level.quantity,
                        Some(acc) => {
                            self.pending = Some(Level { price, quantity: level.quantity });
                            return Some(acc);
                        }
                        None => self.pending = Some(Level { price, quantity: level.quantity }),
                    }
                }
                None => return self.pending.take(),
            }
        }
    }
}
```

Алгоритм здесь записан не так, как он придуман.
Цикл по входу стал `loop` вокруг `inner.next()`, накопитель переехал в поле `pending`,
а выдача значения превратилась в `return` из середины цикла.

### Решение: цикл вместо структуры

`gen`-блок принимает тот же алгоритм в его исходном виде:

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

Структуры, поля `pending` и внешнего `loop` больше нет.
Накопитель остался локальной переменной, а хвостовая корзина отдаётся строкой после цикла.
На выходе всё тот же `impl Iterator`, и на одном стакане оба варианта дают один результат:

```
manual    = [Level { price: 18550, quantity: 3 }, Level { price: 18545, quantity: 9 }, ...]
generated = [Level { price: 18550, quantity: 3 }, Level { price: 18545, quantity: 9 }, ...]
```

Что получаем:
- Алгоритм записан в прямом порядке, а машину состояний под него собирает компилятор.
- Возвращаемый тип остаётся `impl Iterator`, и вызывающий код не меняется.
- Блок годится в тело метода трейта: `fn aggregated(&self) -> impl Iterator<Item = Level>`.

### Хорошие практики

**`gen`-блок ленив, как и любой итератор.**
Тело не выполняется, пока не позвали `next`.
На стакане из четырёх уровней до первого `next` не прочитан ни один,
а после первого прочитаны два: ровно столько, сколько нужно, чтобы отдать первую корзину.
Побочные эффекты внутри `gen` случаются в момент чтения, а не в момент создания.

**`gen move` и заимствование.**
Без `move` блок заимствует то, что использует, и итератор не переживёт эти заимствования.
С `move` он забирает значения себе, и его можно вернуть из функции.
Замыкания подчиняются этому правилу так же.

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
Если состояние укладывается в одну переменную и на каждый вызов отдаётся один элемент,
`std::iter::from_fn` с замыканием решает ту же задачу на стабильном Rust.
`gen` окупается там, где в алгоритме есть цикл, ветвление и несколько точек выдачи.

### Статус фичи

- `#![feature(gen_blocks)]`, RFC 3513 принят 07.04.2024.
- Слово `gen` зарезервировано редакцией 2024.
В редакции 2021 тот же блок не собирается: `gen` там обычный идентификатор,
а `yield` уже зарезервирован, и компилятор отвечает
`expected identifier, found reserved keyword yield`.
- Кроме блоков под той же фичей работают `gen fn` и `async gen`:
первая описывает генератор функцией целиком,
вторая даёт `impl AsyncIterator` для асинхронных потоков.

## Pattern types

Все три части подряд ограничение жило в обёртке: `Price` это `Decimal` за приватным полем,
`OrderBook<DEPTH>` это массив плюс отдельный трейт `SupportedDepth`.
Pattern types обещают другое: диапазон записывается в самом типе значения,
и обёртка не нужна.

Тип объявляется макросом `std::pat::pattern_type!`:

```rust
#![feature(pattern_types, pattern_type_macro)]
use std::pat::pattern_type;

/// Глубина стакана из части 2, но ограничение теперь в типе значения.
type Depth = pattern_type!(usize is 1..);

struct Snapshot { depth: Depth }
```

Литерал вне диапазона отклоняется при компиляции, и `OrderBook<0>` из части 2
здесь невыразим без всякого `SupportedDepth`:

```rust
let s = Snapshot { depth: 10 };   // компилируется
let s = Snapshot { depth: 0 };
// error[E0308]: mismatched types
//    expected `pattern_type!(usize is 1..)`, found integer
```

Диапазон известен компилятору, поэтому у типа есть ниша, и обёртка в `Option` бесплатна:

```
size_of Option<Depth> = 8, Option<usize> = 16, usize = 8
```

Дальше начинаются ограничения, и их больше, чем возможностей.

Значение попадает в такой тип только литералом.
Глубина из конфига это обычный `usize`, и присвоить его нельзя:

```rust
let n: usize = 10;
let s = Snapshot { depth: n };
// error[E0308]: expected `pattern_type!(usize is 1..)`, found `usize`
```

Обратно тоже: `s.depth` не присвоить в `usize` и не разобрать по `match`,
обе попытки дают ту же E0308.
Остаётся `transmute`, а он переносит обещание про диапазон с компилятора на программиста.

Базовые типы ограничены целыми и `char`:

```rust
type Bad = pattern_type!(f64 is 1.0..);
// error[E0277]: `f64` is not a valid base type for range patterns
//    only integer types and `char` are supported
```

Значит, `Price` на `Decimal` из части 1 таким способом не описать в принципе,
и смарт-конструктор остаётся на месте.
Сегодня pattern types закрывают узкий случай: константы в коде,
у которых диапазон известен и проверяется при компиляции.
Всё, что приходит из сети или из конфига, по-прежнему проходит через проверку в рантайме.

### Статус фичи

- `#![feature(pattern_types)]` и `#![feature(pattern_type_macro)]`, issue rust-lang/rust#123646.
- Первая фича помечена внутренней:
`the feature pattern_types is internal to the compiler or standard library`,
и компилятор добавляет `using it is strongly discouraged`.
- Формат предложения это MCP в репозитории types-team, а не RFC.
Внутри компилятора и `core` фича уже работает: через неё описан `*const T is !null`,
и на ней держится ниша у указателей.
- Синтаксис `u32 is 1..` без макроса, который часто встречается в постах,
на nightly от 2026-08-31 не парсится.

## Never type

Часть 1 объявляла `ClientOrderId` с `type Err = Infallible`, потому что `!`
в произвольных позициях типа был за feature-флагом.
В Rust 1.100 он стабилен, и то же самое пишется прямо:

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

Симулятор части 2 отказать не может, и его `type Error` тоже становится `!`.
Тогда результат разбирается неопровержимым паттерном, без `match` и без `unwrap`:

```rust
let Ok(SimOrderId(n)) = sim.submit_order();
```

В размерах это видно по цифрам: `Option<!>` занимает ноль байтов,
а `Result<u64, !>` ровно столько же, сколько сам `u64`, потому что второй вариант невозможен.

`Infallible` в том же релизе перестал быть отдельным типом и стал псевдонимом:

```rust
pub type Infallible = !;
```

Код частей 1 и 2 после этого продолжает работать без правок,
`Result<T, Infallible>` и `Result<T, !>` это один тип,
а документация `std` советует писать `!` напрямую.

### Статус фичи

- Стабилен в Rust 1.100.
На stable 1.98 тот же код отвечает `error[E0658]: the ! type is experimental`.
- `Infallible` остаётся в `std` навсегда как псевдоним,
так что менять код ради нового синтаксиса нужно только там, где важна читаемость.
- Путь к стабилизации у этой фичи самый долгий из пяти:
`!` как тип расходящейся функции работал с версии 1.0,
а в произвольной позиции типа ждал стабилизации больше десяти лет.

## Итог части 4 и что дальше

Части 1–3 обходились стабильным Rust, и у каждого обхода была цена.
Здесь мы посмотрели, что с этими обходами делает nightly, и ответ у пяти фич разный.

Три фичи закрывают долги предыдущих частей прямо сейчас.
`generic_const_exprs` превращает `N <= MAX_BATCH` в обычный bound,
и промах ловится на `cargo check`.
Const traits добавляют `const impl` и bound `[const]`,
после чего смарт-конструктор части 1 проверяет литералы при сборке.
Gen-блоки убирают ручную структуру из-под итератора с накопителем:
алгоритм остаётся циклом, а состояние между вызовами `next` хранит сам блок.

Оставшиеся две в эту схему не попадают, каждая по своей причине.
Pattern types на сегодня принимают только литералы целых чисел,
поэтому ни глубину из конфига, ни цену на `Decimal` они не описывают.
Never type успел стабилизироваться: он в stable с версии 1.100,
а `Infallible` части 1 стал его псевдонимом.

Общая цена у всего этого одна: код под `#![feature]` привязан к дате компилятора.
За последний год `impl const Trait` сменился на `const impl`, `~const` на `[const]`,
а сравнение в bound-е осталось за фичей, которую в текущем виде стабилизировать не планируют.
Отсюда практика: пинуйте nightly по дате в `rust-toolchain.toml`,
держите фичу за границей, которую контролируете сами,
и покрывайте доктестами каждое «это не компилируется», чтобы смена синтаксиса
обнаружилась на сборке примеров, а не в чужом проекте.

Брать ли nightly в работу, зависит от того, что именно вы им закрываете.
Проверка литералов при сборке и итератор без ручной структуры окупаются сразу.
А вот ограничения прямо в типе значения ждать пока рано:
pattern types сегодня решают задачу меньше той, что закрывает обёртка со смарт-конструктором.

Все четыре части описывали правила, которые проверяются на данных или при сборке.
Есть класс правил, которых в этом списке не было: обязательства.
Заявку, поставленную в стакан, надо либо исполнить, либо отменить,
а зарезервированную маржу освободить, и Rust сегодня умеет только половину этого:
значение нельзя использовать дважды, но уронить молча можно.
В части 5 посмотрим на substructural types, где «ровно один раз» становится частью типа,
на эффекты, где одна и та же торговая логика запускается в бэктесте и в проде
с разными гарантиями, на variadic generics, которые заменят HList части 3 нативной записью,
и на view types, где в типе видно, какие поля структуры заимствованы.
