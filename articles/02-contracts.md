# Type-driven development в Rust. Часть 2/5: задаём контракты между компонентами — traits, associated types, const generics

В части 1 типы отвечали за состояние: какие значения возможны, какие комбинации допустимы, в каком порядке идут переходы.
Но кроме данных в программе есть компоненты, которые общаются между собой: заявка уходит на биржу, котировки приходят с рынка и так далее.
У каждой такой границы есть контракт — чего одна сторона ждёт и что другая обещает.

В Rust контракт задают трейтом.
Возьмём код из части 1: `DraftOrder::submit` отправлял заявку «на биржу» — но на какую?
Там мы об этом умолчали: биржа была одна и абстрактная.
В жизни их много — Binance, NYSE, крипто-DEX, тестовая песочница, — и у каждой свой способ подключения.
Но суть у всех одна: принять заявку и вернуть либо идентификатор, либо отказ.

## Трейты как контракты

### Проблема: захардкоженная биржа

Пока биржа одна, всё просто — зовём её напрямую:

```rust
fn place_order(order: DraftOrder, binance: &Binance) -> Result<BinanceOrderId, BinanceError> {
    binance.submit(order)
}
```

Появилась вторая площадка — и простота кончилась.
Ленивый ход — `enum` по биржам и `match` на каждом вызове:

```rust
enum Exchange {
    Binance(Binance),
    Nyse(Nyse),
}

fn route(order: DraftOrder, exchange: &Exchange) -> Result</* ??? */, /* ??? */> {
    match exchange {
        Exchange::Binance(b) => b.submit(order),
        Exchange::Nyse(n) => n.submit(order),
    }
}
```

Затык виден уже на типе возврата: у Binance свой `OrderId` и свой `Error`, у NYSE — свои, а `match` обязан вернуть что-то одно.
Каждая новая площадка — правка всех таких `match`-ей, а они расползутся по коду ровно так, как мы видели в части 1.
Плюс список бирж зашит в `enum`: пользователь вашей библиотеки не подключит свою площадку, не залезая в исходник.

`enum` здесь не виноват — просто это не его задача.
В части 1 он отлично кодировал «одно из» для данных с закрытым набором форм.
А тут набор открытый и растущий: бирж сколько угодно, и часть из них появится уже после того, как вы выложили код.

### Решение: трейт

Вынесем роль в трейт — контракт «умею принять заявку»:

```rust
trait ExchangeConnector {
    fn submit(&self, order: DraftOrder) -> Result<OrderId, ExchangeError>;
}
```

(`OrderId` и `ExchangeError` пока общие на всех — что с этим не так и как сделать их «своими на каждой бирже», разберём в разделе про ассоциированные типы.)
Это нижний слой `DraftOrder::submit` из части 1: коннектор отдаёт сырой `OrderId` биржи, а `WorkingOrder` из него собирает уже сам `submit`.

Каждая площадка выполняет контракт по-своему:

```rust
struct Binance { /* http-клиент, ключи */ }
struct Nyse { /* FIX-сессия */ }

impl ExchangeConnector for Binance {
    fn submit(&self, order: DraftOrder) -> Result<OrderId, ExchangeError> { /* ... */ }
}

impl ExchangeConnector for Nyse {
    fn submit(&self, order: DraftOrder) -> Result<OrderId, ExchangeError> { /* ... */ }
}
```

А код, который ставит заявку, про конкретные биржи больше не знает. Он работает с любым, кто выполнил контракт:

```rust
fn route<C: ExchangeConnector>(order: DraftOrder, exchange: &C) -> Result<OrderId, ExchangeError> {
    exchange.submit(order)
}
```

`<C: ExchangeConnector>` читается как «для любого `C`, который реализует `ExchangeConnector`».
Ни `match`, ни зашитого списка площадок.
Новая биржа — это новый `impl`, и `route` подхватит её без единой правки; свою площадку добавит даже пользователь библиотеки.

<!-- ============================================================
ДАЛЬШЕ ПО ПЛАНУ (черновик, ещё не написано — пишем секцию за секцией):

- ### Хорошие практики (к секции «Трейты»): trait bounds и `where`, дефолтные методы,
  статическая диспетчеризация vs `dyn ExchangeConnector` (когда что), sealed traits.
- ## Ассоциированные типы
    Проблема: у каждой биржи свой OrderId/Error — либо взрыв generic-параметров
    (Connector<Id, Err>), либо всё стирается в String. Решение: type OrderId; type Error;
    (выходные типы привязаны к impl, а не выбираются вызывающим). Контраст input vs output.
- ## Const generics
    Хук: OrderBook<const DEPTH: usize> { bids: [Level; DEPTH], asks: [Level; DEPTH] }.
    Где упирается в generic_const_exprs — мостик в Часть 4.
- ## Кульминация: CGP — 1-2 показательных сниппета.
- ## Итог + мост в Часть 3 (валидаторы).

CGP-материал для финала:
CGP (Context-Generic Programming) — https://github.com/contextgeneric/cgp — доведение
trait + associated types + GAT до композиционной парадигмы. Stable Rust 1.81+, используется
в IBC-relayer (Hermes). Доки: https://contextgeneric.dev. Книга «Context-Generic Programming
Patterns» — основной источник. Подача: 1-2 сниппета как кульминация, без внутренностей;
развёрнуто — ссылкой на книгу.
============================================================ -->