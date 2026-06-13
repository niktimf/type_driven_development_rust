# Type-driven development в Rust. Часть 2/5: задаём контракты между компонентами — трейты, associated types, const generics

_Черновик в работе._

Планируется:
- Трейты как контракты на поведение
- Ассоциированные типы
- Const generics
- Закрывающий кейс: **CGP (Context-Generic Programming)** — https://github.com/contextgeneric/cgp — как пример доведения trait + associated types + GAT до полноценной композиционной парадигмы. Работает на stable Rust 1.81+, используется в IBC-relayer (Hermes). Доки: https://contextgeneric.dev. Книга «Context-Generic Programming Patterns» — основной источник.
  - Идея для подачи: показать 1-2 показательных сниппета как кульминацию раздела про трейты и assoc types, без углубления во внутренности. Развёрнутый разбор — отдельной статьёй или ссылкой на книгу авторов.