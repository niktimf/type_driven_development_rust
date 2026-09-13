# Type-driven development в Rust. Часть 4/5: выходим за стабильный Rust — generic const exprs, const traits, gen blocks, pattern types

«Жизнь есть только на nightly» в сообществе Rust говорят давно и обычно как шутку,
но в каждой шутке есть доля правды:
самые свежие и интересные фичи компилятора появляются именно там.
Для системы типов это не шутка: её фичи ждут стабилизации дольше всего.
`!` в позиции типа шёл к stable больше десяти лет, стабилизацию для 1.41 в 2019-м откатили,
не дожидаясь релиза, а specialization с 2016 года так и не вышла.
Кто выражает инварианты в типах, рано или поздно пишет `#![feature]` в первой строке крейта,
и эта часть о том, что за ней сейчас.

Части 1–3 обходились stable, и там, где его не хватало, использовали обходные решения:
`const`-блок вместо `N <= MAX_BATCH` в bound-е, ручная структура под итератор вместо цикла.
В этой части берём тот же код и смотрим, что с ним делает nightly.

Всё проверено на nightly от 2026-08-31 (rustc 1.100.0-nightly).
Крейт примеров пинует эту дату в `rust-toolchain.toml`, и все «компилятор говорит» ниже
относятся к ней.

У const traits и pattern types синтаксис за последний год менялся,
и код из прошлогодних постов на этом nightly уже не парсится,
так что примеры под `#![feature]` придётся править вслед за компилятором.

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

Такая проверка срабатывает после мономорфизации,
когда `const`-блок вычисляется при инстанцировании функции под конкретные `EC` и `N`:

```
$ cargo check      # проходит
$ cargo build
error[E0080]: evaluation panicked: batch exceeds exchange limit
   = note: while instantiating `submit_batch_typed::<RestExchange, 10>`
```

`cargo check` проходит, ошибка приходит не на вызов, а на инстанцирование,
и если функция лежит в библиотеке, первым её увидит тот, кто библиотеку подключил.

### Решение: условие в bound-е

Раз `where` принимает только bound-ы, условие надо сделать bound-ом.
Для этого нужен тип с параметром `bool`, у которого реализация трейта есть только для `true`:

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

`{ N <= EC::MAX_BATCH }` — это константное выражение над generic-параметрами,
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
Bound на выражении в тело не убрать: он стоит в сигнатуре,
и фича из детали реализации становится требованием ко всем, кто библиотеку подключает.

**Один `Assert` на одно условие.**
`Assert<{ .. }>: IsTrue` в ошибке печатается как `expected false, found true`,
условие в тексте не названо.
Если условий в одной сигнатуре несколько, из сообщения не понять, какое сработало.
Держите условие в отдельной строке `where` и сразу над ним комментарий,
на который читатель попадёт по `required by a bound`.

**Предупреждение `incomplete_features` стоит прочитать.**
Без `#![allow(incomplete_features)]` компилятор сообщает,
что фича неполна и может привести к его падению.
Bound-ы на выражениях стоит держать на границе, которую контролируете сами,
и не выносить в публичный API.

### Статус фичи

- `#![feature(generic_const_exprs)]`, tracking issue rust-lang/rust#76560,
помечена `incomplete_features`.
- На nightly от 2026-08-31 не работает с новым trait solver: компилятор пишет
`feature(generic_const_exprs) is not supported with the next-generation trait solver`
и сам откатывает крейт на старый solver.
- Стабилизация в текущем виде не планируется.
Вместо неё в компиляторе готовят уменьшенное подмножество, `min_generic_const_args`:
там const-аргументом будет путь вроде `EC::MAX_BATCH`, а не выражение над параметрами.
На nightly от 2026-08-31 это ещё заготовка: и путь `Batch<E::MAX_BATCH>` (E0747),
и выражение `{ N <= E::MAX_BATCH }` под ней отклоняются.
Сравнение из этого раздела в подмножество не входит по замыслу,
так что bound на выражении останется под `generic_const_exprs`.

## Const traits

`const fn` на стабильном Rust может вызывать другую `const fn`, но не метод трейта.
Const traits снимают это ограничение: трейт объявляется `const`,
реализация тоже, и тогда её методы работают в `const`-контексте.

### Проблема: соотношения между потолками проверяет только ревью

Лимиты части 3, по номиналу заявки и по позиции, в проде приходят из конфига
и проверяются в рантайме.
Но у конфига есть потолок, зашитый в код намеренно:
опустить лимит конфигом можно, поднять выше потолка нельзя,
а изменение самого потолка проходит через ревью и деплой.
Так же устроены лимиты потерь: на одну сделку, порог предупреждения за день и остановка за день.
В коде их потолки — литералы по замыслу:

```rust
const MAX_LOSS_PER_TRADE: Money<Usd> = usd(dec!(50_000.00));
const WARN_DAILY_LOSS: Money<Usd> = usd(dec!(750_000.00));
const MAX_DAILY_LOSS: Money<Usd> = usd(dec!(1_000_000.00));
```

Сами литералы проверяются при сборке уже на stable.
`Money::new` — это `const fn`, которая возвращает `Result`.
`usd` разбирает результат `match`-ем и на `Err` паникует,
а паника в `const`-контексте — это ошибка сборки:

```rust
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

Соотношения между потолками через `<` на stable уже не проверить.
Предупреждение должно срабатывать раньше остановки, и это хочется записать `assert!`-ом,
но `<` на `Money` — это `PartialOrd::lt`, метод трейта,
и с обычной реализацией `PartialOrd` в `const`-контексте его вызвать нельзя:

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

Компилятор называет недостающее по имени, `const PartialOrd`, и показывает, куда его дописать.

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
Масштаб у всех `Money<C>` одной валюты общий, его проверил конструктор,
поэтому сравниваются мантиссы: собственное сравнение `Decimal` не `const`:

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
Так пишется общая проверка, что ступени идут по возрастанию.
В `const` она сравнивает потолки, в рантайме той же функцией проверяют ступени из конфига:

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

`assert!` из «Проблемы» теперь собирается.
Дневной потолок при этом выводится из потолка на сделку, без отдельного литерала.
Умножение на целое — это `Mul::mul`, ещё один метод трейта, и `const impl` для него пишется так же.
Масштаб оно не меняет: мантисса умножается, `scale` остаётся, и сравнение мантисс остаётся верным:

```rust
const impl<C> Mul<u32> for Money<C> {
    type Output = Money<C>;
    fn mul(self, n: u32) -> Money<C> {
        // from_i128_with_scale у Decimal не const: мантисса собирается через from_parts
        let amount = from_mantissa(self.amount.mantissa() * n as i128, self.amount.scale());
        Money { amount, _currency: PhantomData }
    }
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

Что получаем:
- Соотношения между потолками проверены при сборке,
и сообщение из `assert!` попадает в ошибку компиляции.
- Реализация одна на два контекста:
тем же `<` лимит из конфига сравнивается с потолком в рантайме.
- Арифметика на newtype при сборке:
производный потолок считается через `*` из потолка на сделку.

### Хорошие практики

**`[const] Trait` в bound-е, `const trait` в объявлении.**
Первое читается «константная реализация нужна только в `const`-контексте»,
второе разрешает реализациям быть константными вообще.
Без `[const]` тело `ascending` не собирается, хотя `T: PartialOrd` на месте:
компилятор отвечает `the trait bound T: [const] PartialOrd is not satisfied` на строке с `<`.

**Константной становится не вся библиотека.**
`Decimal` даёт в `const` только `mantissa()`, `scale()` и конструктор `from_parts`,
сравнения и арифметики там нет, отсюда и сравнение мантисс, и `from_mantissa` в умножении.
Сравнение мантисс верно только потому, что `Money::new` уравнял масштаб по `MINOR_UNITS`,
а поле приватное, и обойти конструктор нельзя, как у `Price` в части 1.
Арифметику в `const` должен добавить сам `rust_decimal`, фича здесь ни при чём.

**Фич нужно три.**
`const_trait_impl` даёт синтаксис,
`const_cmp` разрешает `const impl` для `PartialEq` и `PartialOrd`,
`const_ops` — для `Add`, `Mul` и остальных операторов.
Без двух последних `const impl` для трейта из `std` не собирается даже для своего типа:
`E0658: use of unstable const library feature const_cmp`, `trait is not stable as const yet`.

### Статус фичи

- `#![feature(const_trait_impl)]`, tracking issue rust-lang/rust#67792.
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
и GAT из сигнатуры убирается на стабильном Rust.

```rust
pub trait MarketDataFeed {
    fn bids(&self) -> impl Iterator<Item = &Level<Self::Quote>>;
}
```

Ассоциированный тип исчез вместе с параметром времени жизни, и реализация осталась той же строкой.

Другое дело, когда итератор нужно построить поверх фида.
Стратегии смотрят не на сырой стакан, а на агрегированный: уровни собирают в корзины по шагу цены,
объёмы внутри корзины складывают.
Такому итератору нужно состояние: накопленная корзина.
Её отдают, когда цена перешла границу, и ещё раз в конце, когда уровни кончились.

Комбинаторы под это не подходят.
`map` и `scan` на каждый входной элемент отдают ровно один выходной,
а при агрегации несколько уровней складываются в одну корзину,
и выходных элементов меньше, чем входных.
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

Сам алгоритм — это один проход по уровням: цикл, накопитель внутри него
и два места, где корзину отдают.
В ручном `Iterator` каждый вызов `next` входит в тело заново, и запись меняется:
цикл по входу стал `loop` вокруг `inner.next()`, накопитель переехал в поле `pending`,
а «отдать корзину» превратилось в `return` из середины цикла.

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
- Блок можно записать в тело метода трейта: `fn aggregated(&self) -> impl Iterator<Item = Level>`.

### Хорошие практики

**`gen`-блок ленивый, как и любой итератор.**
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

Pattern type — это базовый тип плюс паттерн, которому обязано соответствовать значение.
`usize is 1..` — все `usize`, кроме нуля: множество значений сужено предикатом,
а представление в памяти то же, что у `usize`.
В теории типов ближайшее имя — refinement types, уточняющие типы,
но уточнение здесь записывается паттерном из `match`, отсюда и название,
и подтипом базового типа pattern type не считается: это отдельный тип.

В частях 1–3 такое ограничение записывалось обёрткой: `Price` — это `Decimal` за приватным полем,
`OrderBook<DEPTH>` — массив плюс отдельный трейт `SupportedDepth`.
С pattern type диапазон стоит в самом типе значения, обёртка не нужна,
а объявляется тип макросом `std::pat::pattern_type!`:

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

Ноль в `usize is 1..` невозможен, и компилятор отдаёт его под `None`:
`Option<Depth>` занимает столько же, сколько сам `usize`, как и `Option<NonZeroUsize>`:

```
size_of Option<Depth> = 8, Option<usize> = 16, usize = 8
```

Значение попадает в такой тип только литералом.
Глубина из конфига — это обычный `usize`, и присвоить его нельзя:

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
- Формат предложения — это MCP в репозитории types-team, а не RFC.
Внутри компилятора и `core` фича уже работает: через неё описан `*const T is !null`,
и нулевой адрес там отдан под `None` в `Option<NonNull<T>>`.
- Синтаксис `u32 is 1..` без макроса, который часто встречается в постах,
на nightly от 2026-08-31 не парсится.

## Never type

Часть 1 объявляла `ClientOrderId` с `type Err = Infallible`, потому что `!`
в произвольных позициях типа был за feature-флагом.
С 1.100 он стабилен, и то же самое пишется прямо:

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
Тогда результат разбирается, без `match` и без `unwrap`:

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
`Result<T, Infallible>` и `Result<T, !>` — это один тип,
а документация `std` советует писать `!` напрямую.

### Статус фичи

- Стабилизирован 24.08.2026 (PR rust-lang/rust#155499), в stable выходит с 1.100.
На stable 1.98 тот же код отвечает `error[E0658]: the ! type is experimental`.
- `Infallible` остаётся в `std` навсегда как псевдоним,
так что менять код ради нового синтаксиса нужно только там, где важна читаемость.
- Путь к стабилизации у этой фичи самый долгий из пяти:
`!` как тип расходящейся функции работал с версии 1.0,
а в произвольной позиции типа — только с 1.100.
Попыток стабилизации, по счёту автора влитого PR, было шесть;
ту, что для 1.41, откатили из-за never-type fallback.

## Итог части 4 и что дальше

Три фичи из пяти закрывают долги частей 1–3 уже на текущем nightly.
Pattern types этого не делают: они принимают только литералы целых чисел,
поэтому ни глубину из конфига, ни цену на `Decimal` ими не описать.
Never type за время работы над серией успели стабилизировать,
и `Infallible` части 1 стал его псевдонимом.

Весь код под `#![feature]` привязан к дате компилятора.
За последний год `impl const Trait` сменился на `const impl`, `~const` на `[const]`,
а сравнение в bound-е осталось за фичей, которую в текущем виде стабилизировать не планируют.
Отсюда практика: nightly пинуется по дате в `rust-toolchain.toml`,
а фича не выходит за границу, которую вы контролируете сами.
Каждое «это не компилируется» стоит покрыть доктестом:
тогда смена синтаксиса обнаружится на сборке примеров, а не в чужом проекте.

Брать ли nightly в работу, зависит от того, что именно вы им закрываете.
Потолки лимитов, проверенные при сборке, и итератор без ручной структуры окупаются сразу.
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
