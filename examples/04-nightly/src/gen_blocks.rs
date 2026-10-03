//! Gen-блоки: итератор с состоянием записывается циклом с `yield`,
//! а машину состояний под него собирает компилятор.
//!
//! В разделе два шага. Первый работает на stable: `impl Trait` в возвращаемом
//! типе метода трейта (с 1.75) убирает GAT `Levels<'a>` из сигнатуры фида части 2,
//! см. [`Feed`]. Второй требует `gen_blocks`: агрегированный стакан, где несколько
//! уровней складываются в одну корзину, а хвостовая корзина отдаётся уже после
//! конца входа, см. [`Aggregated`] и [`aggregate`].

use tdd_02_contracts::domain::Currency;
use tdd_02_contracts::feed::{BookFeed, Level as FeedLevel, MarketDataFeed};

/// Контракт фида части 2 без GAT: `impl Trait` в возвращаемом типе метода
/// трейта стабилен с 1.75, и параметр времени жизни у ассоциированного типа
/// больше не нужен. Реализация для `BookFeed` — та же строка, что в части 2.
pub trait Feed {
    type Quote: Currency;

    fn bids(&self) -> impl Iterator<Item = &FeedLevel<Self::Quote>>;
}

impl<Quote: Currency> Feed for BookFeed<Quote> {
    type Quote = Quote;

    fn bids(&self) -> impl Iterator<Item = &FeedLevel<Quote>> {
        MarketDataFeed::bids(self)
    }
}

/// Уровень стакана в целых тиках и лотах: корзина по шагу цены считается
/// целочисленным делением, поэтому здесь `u64`, а не `Price<Quote>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Level {
    pub price: u64,
    pub quantity: u64,
}

enum AggregationState {
    Empty,
    Accumulating(Level),
    Finished,
}

/// Агрегация, записанная руками: каждый вызов `next` входит в тело заново.
/// Поле `state` хранит состояние и накопленный интервал между вызовами,
/// а выдача интервала завершает вызов через `return`.
pub struct Aggregated<I: Iterator<Item = Level>> {
    inner: I,
    bucket: u64,
    state: AggregationState,
}

impl<I: Iterator<Item = Level>> Aggregated<I> {
    pub fn new(inner: I, bucket: u64) -> Self {
        Self {
            inner,
            bucket,
            state: AggregationState::Empty,
        }
    }
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

/// Та же агрегация `gen`-блоком: один проход по уровням, накопитель — локальная
/// переменная, хвостовая корзина отдаётся строкой после цикла.
///
/// Блок ленивый: тело не выполняется, пока не позвали `next`, см. тест
/// `gen_block_reads_input_lazily`. `move` задаёт захват переменных по значению:
/// без него `bucket` заимствуется и итератор нельзя вернуть из функции.
///
/// `?` внутри блока не работает: оператор требует, чтобы `Result` или `Option`
/// возвращал сам блок, а `gen`-блок возвращает `()`:
///
/// ```compile_fail,E0277
/// #![feature(gen_blocks)]
///
/// // error[E0277]: the `?` operator can only be used in a gen block that returns
/// //               `Result` or `Option` (or another type that implements `FromResidual`)
/// fn parsed(items: Vec<String>) -> impl Iterator<Item = u64> {
///     gen move {
///         for s in items {
///             yield s.parse::<u64>()?;
///         }
///     }
/// }
/// ```
///
/// В редакции 2021 `gen` — обычный идентификатор, и блок не парсится:
///
/// ```compile_fail,edition2021
/// #![feature(gen_blocks)]
///
/// fn evens() -> impl Iterator<Item = u32> {
///     gen move {
///         yield 2;
///     }
/// }
/// ```
pub fn aggregate(levels: impl Iterator<Item = Level>, bucket: u64) -> impl Iterator<Item = Level> {
    gen move {
        let mut acc: Option<Level> = None;
        for level in levels {
            let price = level.price - level.price % bucket;
            match acc {
                Some(ref mut a) if a.price == price => a.quantity += level.quantity,
                Some(a) => {
                    yield a;
                    acc = Some(Level {
                        price,
                        quantity: level.quantity,
                    });
                }
                None => {
                    acc = Some(Level {
                        price,
                        quantity: level.quantity,
                    })
                }
            }
        }
        if let Some(a) = acc {
            yield a;
        }
    }
}

/// `gen`-блок можно использовать в теле метода трейта: возвращаемый тип остаётся
/// `impl Iterator`, как у `bids` в [`Feed`].
pub trait AggregatedBook {
    fn aggregated(&self, bucket: u64) -> impl Iterator<Item = Level>;
}

impl AggregatedBook for Vec<Level> {
    fn aggregated(&self, bucket: u64) -> impl Iterator<Item = Level> {
        aggregate(self.iter().copied(), bucket)
    }
}

/// Под той же фичей есть `gen fn`: генератор описывается функцией целиком,
/// тип после `->` — это тип элемента, а возвращается `impl Iterator`.
pub gen fn price_grid(from: u64, to: u64, tick: u64) -> u64 {
    let mut price = from;
    while price <= to {
        yield price;
        price += tick;
    }
}

/// Ошибки из `gen`-блока отдают элементами: `yield Err(..)`, а вызывающий
/// собирает их через `collect::<Result<Vec<_>, _>>()`.
pub fn parse_levels(
    lines: Vec<String>,
) -> impl Iterator<Item = Result<Level, std::num::ParseIntError>> {
    gen move {
        for line in lines {
            let mut parts = line.split(':');
            let price = parts.next().unwrap_or("").trim().parse::<u64>();
            let quantity = parts.next().unwrap_or("").trim().parse::<u64>();
            yield match (price, quantity) {
                (Ok(price), Ok(quantity)) => Ok(Level { price, quantity }),
                (Err(e), _) | (_, Err(e)) => Err(e),
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    /// Четыре уровня, шаг корзины 5: первые два попадают в разные корзины,
    /// вторая корзина собирает два уровня.
    fn book() -> [Level; 4] {
        [
            Level {
                price: 18550,
                quantity: 3,
            },
            Level {
                price: 18547,
                quantity: 4,
            },
            Level {
                price: 18546,
                quantity: 5,
            },
            Level {
                price: 18542,
                quantity: 2,
            },
        ]
    }

    #[test]
    fn manual_and_gen_agree() {
        let manual: Vec<Level> = Aggregated::new(book().into_iter(), 5).collect();
        let generated: Vec<Level> = aggregate(book().into_iter(), 5).collect();
        let expected = vec![
            Level {
                price: 18550,
                quantity: 3,
            },
            Level {
                price: 18545,
                quantity: 9,
            },
            Level {
                price: 18540,
                quantity: 2,
            },
        ];
        assert_eq!(manual, expected);
        assert_eq!(generated, expected);
    }

    #[test]
    fn manual_empty_input_stays_finished() {
        let mut aggregated = Aggregated::new(std::iter::empty(), 5);
        assert_eq!(aggregated.next(), None);
        assert_eq!(aggregated.next(), None);
    }

    #[test]
    fn manual_emits_last_bucket_once_and_stops_reading() {
        let reads = Cell::new(0);
        let mut input = [
            Some(Level {
                price: 18547,
                quantity: 4,
            }),
            None,
            Some(Level {
                price: 18542,
                quantity: 2,
            }),
        ]
        .into_iter();
        let levels = std::iter::from_fn(|| {
            reads.set(reads.get() + 1);
            input.next().unwrap_or(None)
        });
        let mut aggregated = Aggregated::new(levels, 5);

        assert_eq!(
            aggregated.next(),
            Some(Level {
                price: 18545,
                quantity: 4
            })
        );
        assert_eq!(reads.get(), 2);
        assert_eq!(aggregated.next(), None);
        assert_eq!(aggregated.next(), None);
        assert_eq!(reads.get(), 2);
    }

    #[test]
    fn gen_block_reads_input_lazily() {
        let reads = Cell::new(0);
        let counted = book().into_iter().inspect(|_| reads.set(reads.get() + 1));
        let mut aggregated = aggregate(counted, 5);
        assert_eq!(
            reads.get(),
            0,
            "до первого next не прочитан ни один уровень"
        );
        let first = aggregated.next();
        assert_eq!(
            first,
            Some(Level {
                price: 18550,
                quantity: 3
            })
        );
        assert_eq!(reads.get(), 2, "для первой корзины хватило двух уровней");
    }

    #[test]
    fn gen_block_inside_a_trait_method() {
        let book: Vec<Level> = book().to_vec();
        assert_eq!(book.aggregated(5).count(), 3);
    }

    #[test]
    fn gen_fn_is_a_generator_written_as_a_function() {
        assert_eq!(
            price_grid(18540, 18550, 5).collect::<Vec<_>>(),
            vec![18540, 18545, 18550]
        );
    }

    #[test]
    fn errors_are_yielded_as_items() {
        let ok: Result<Vec<Level>, _> =
            parse_levels(vec!["18550: 3".into(), "18547: 4".into()]).collect();
        assert_eq!(ok.unwrap().len(), 2);
        let bad: Result<Vec<Level>, _> =
            parse_levels(vec!["18550: 3".into(), "x: 4".into()]).collect();
        assert!(bad.is_err());
    }

    #[test]
    fn feed_without_gat_yields_the_same_levels() {
        use rust_decimal::Decimal;
        use tdd_02_contracts::domain::Usd;
        use tdd_02_contracts::order::tests_support::spec;

        let mut feed = BookFeed::<Usd>::default();
        let spec = spec();
        feed.push_bid(FeedLevel::new(
            spec.price(Decimal::new(18550, 2)).unwrap(),
            spec.quantity(Decimal::from(10)).unwrap(),
        ));
        assert_eq!(Feed::bids(&feed).count(), 1);
        assert_eq!(MarketDataFeed::bids(&feed).count(), 1);
    }
}
