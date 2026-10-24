# Comparisons.RedisVSDoublets ([english version](README.md))

[![Rust](https://github.com/linksplatform/Comparisons.RedisVSDoublets/actions/workflows/rust.yml/badge.svg)](https://github.com/linksplatform/Comparisons.RedisVSDoublets/actions/workflows/rust.yml)
[![C#](https://github.com/linksplatform/Comparisons.RedisVSDoublets/actions/workflows/csharp.yml/badge.svg)](https://github.com/linksplatform/Comparisons.RedisVSDoublets/actions/workflows/csharp.yml)
[![Benchmarks](https://github.com/linksplatform/Comparisons.RedisVSDoublets/actions/workflows/benchmarks.yml/badge.svg)](https://github.com/linksplatform/Comparisons.RedisVSDoublets/actions/workflows/benchmarks.yml)
[![CI quality](https://github.com/linksplatform/Comparisons.RedisVSDoublets/actions/workflows/quality.yml/badge.svg)](https://github.com/linksplatform/Comparisons.RedisVSDoublets/actions/workflows/quality.yml)

Сравнение [Redis](https://redis.io) и Doublets (связей) LinksPlatform как
хранилищ в оперативной памяти, на Rust и на C#, сделанное по образцу
[Comparisons.SQLiteVSDoublets](https://github.com/linksplatform/Comparisons.SQLiteVSDoublets)
для [задачи #1](https://github.com/linksplatform/Comparisons.RedisVSDoublets/issues/1).
Doublets работает в памяти, без файлов, в процессе бенчмарка; Redis работает
как локальный сервер без сохранения на диск. Оба используются через нативные
библиотеки, без промежуточного слоя:

| Язык | Клиент Redis | Doublets |
| --- | --- | --- |
| Rust | [redis-rs](https://crates.io/crates/redis) (синхронный) | [doublets](https://crates.io/crates/doublets) 0.8.0 |
| C# | [StackExchange.Redis](https://www.nuget.org/packages/StackExchange.Redis) | [Platform.Data.Doublets](https://www.nuget.org/packages/Platform.Data.Doublets) 0.18.1 |

## Бенчмарки

Есть два набора бенчмарков, и оба языка выполняют одинаковую работу на
одинаковых данных. `B` — число фоновых связей (или ключей, элементов очереди
и элементов множества), которые хранилище содержит перед каждой итерацией, а
`N` — число операций одной итерации.

### Хранение связей

Обе базы данных хранят связи `(id, source, target)` за одним интерфейсом
(трейт `Doublets` библиотеки doublets в Rust, `IBenchedLinks` в C#), и каждое
хранилище начинается с точек `1..=B`:

| Операция | Измеряемая работа одной итерации | Отменяется после итерации |
| --- | --- | --- |
| Create | `N` точек `B + 1..=B + N` | их удалением |
| Update | `N` × (`update(id, 0, 0)`, затем `update(id, id, id)`) | ничем (второе изменение восстанавливает связь) |
| Delete | связи от `B` до `B - N + 1` | их повторным созданием |
| Each All | один перебор всех `B` связей | ничем (только чтение) |
| Each Identity | `N` запросов `[id, *, *]` для id `1..=N` | ничем (только чтение) |
| Each Concrete | `N` запросов `[*, id, id]` | ничем (только чтение) |
| Each Outgoing | `N` запросов `[*, id, *]` | ничем (только чтение) |
| Each Incoming | `N` запросов `[*, *, id]` | ничем (только чтение) |

Redis хранит связь в одном хеше и индексирует её ещё тремя ключами, поэтому
каждый запрос — одна нативная команда чтения, а каждое изменение — один вызов
функции Redis из [`redis/links.lua`](redis/links.lua), общей для обоих языков,
которая атомарно, за один сетевой обмен, изменяет связь и все её индексы:

| Ключ | Тип | Содержимое | Запрос |
| --- | --- | --- | --- |
| `links` | hash | id → `source:target` | `[id, *, *]` = `HGET`, `[*, *, *]` = `HGETALL` |
| `out:{source}` | hash | id → target | `[*, source, *]` = `HGETALL` |
| `in:{target}` | hash | id → source | `[*, *, target]` = `HGETALL` |
| `pair:{source}:{target}` | set | id | `[*, source, target]` = `SMEMBERS` |

Doublets запускает оба своих хранилища в памяти: `Doublets_United_Volatile`
(один массив связей с деревьями индексов) и `Doublets_Split_Volatile`
(отдельные массивы данных и индексов).

### Типичное использование Redis

Структуры данных, для которых обычно используют Redis, реализованные на
связях. Хранилище начинается с ключей `1..=B` (значение каждого — его же
номер), очереди `1, ..., B` и множества `{1, ..., B}`:

| Операция | Измеряемая работа одной итерации | Отменяется после итерации |
| --- | --- | --- |
| KeyValue Get | `N` × `GET k` для ключей `1..=N` | ничем (только чтение) |
| KeyValue Set | `N` × `SET k k+B` для ключей `1..=N` | `SET k k` |
| KeyValue Delete | `N` × `DEL k` для ключей от `B` до `B - N + 1` | `SET k k` |
| Counter Increment | `N` × `INCR k` для ключей `1..=N` | `SET k k` |
| Queue Push | `N` × `RPUSH B+i` | `N` × `RPOP` |
| Queue Pop | `N` × `LPOP`, возвращающих `1..=N` | `LPUSH` от `N` до `1` |
| Set Add | `N` × `SADD B+i` | `SREM B+i` |
| Set Remove | `N` × `SREM m` для элементов от `B` до `B - N + 1` | `SADD m` |
| Set Contains | `N` × `SISMEMBER m` для элементов `1..=N` | ничем (только чтение) |

Redis хранит их в своих нативных типах, поэтому каждая операция — одна
нативная команда: строки `k:{key}`, список `queue` и множество `members`.
Doublets хранит числа как внешние ссылки (сырые числа), которые никогда не
совпадают с id связей, и хранит:

- пару ключ-значение как связь `(key, value)`, поэтому `GET key` — это поиск
  по индексу `(*, key, *)`;
- элемент множества как связь `(set, member)`, где `set` — точка, поэтому
  `SISMEMBER` — это поиск пары `(set, member)`;
- очередь как односвязный список: узел значения — связь `(next, value)`, а
  связь-заголовок `queue = (first, last)` указывает на её концы, поэтому
  `RPUSH` и `LPOP` переходят только по id связей.

Здесь используется только объединённое хранилище: раздельные хранилища обеих
библиотек теряют связи, в которых смешаны id связей и сырые числа (см.
[эксперименты](experiments)). Ключи, значения и элементы — числа от 1 до
`i64::MAX`, наибольшего значения счётчика Redis; хранилища Doublets отклоняют
другие числа.

### Режимы Redis

Каждый бенчмарк Redis выполняется в трёх режимах, чтобы была видна стоимость
сетевых обменов:

| Реализация | Завершённая итерация |
| --- | --- |
| `Redis_NonTransaction` | каждая операция — один сетевой обмен, и её результат известен, когда она возвращает управление |
| `Redis_Transaction` | операции итерации ставятся в очередь `MULTI`/`EXEC` |
| `Redis_Pipeline` | операции итерации отправляются конвейером (pipeline) |

Измеряемое время начинается до начала итерации и заканчивается после её
фиксации, поэтому в режимах транзакции и конвейера оно включает отправку
накопленных команд и получение всех ответов. Каждая найденная связь и каждое
прочитанное значение потребляются (`black_box` в Rust, контрольная сумма в C#).

### Методика

- До измерения хранилище получает фоновые данные, а после каждой итерации
  отмена возвращает его в то же состояние, поэтому каждая итерация начинается
  с одинаковых данных. Ни фоновые данные, ни отмена, ни подключение не
  измеряются.
- Каждый бенчмарк проверяет свои результаты вне измеряемого времени:
  хранилище должно содержать ровно фоновые данные до первой и после последней
  итерации, а одна итерация до измерения должна найти ровно ожидаемые связи
  или значения (их число и сумму их id или значений). Хранилище, которое
  теряет данные или находит неверные результаты, завершает запуск ошибкой
  вместо того, чтобы сообщить время.
- Rust использует [Criterion](https://crates.io/crates/criterion); C# — свой
  измеритель с той же выборкой, который печатает те же строки `bencher`. Для
  Redis берётся 10 плоских выборок за 5 с после 1 с прогрева; для Doublets —
  100 выборок за 2 с после 1 с прогрева. В таблицах — медианное время одной
  итерации.
- Каждая ячейка Doublets сравнивает свою медиану с самым быстрым режимом Redis
  (выделен жирным): `× faster` (быстрее), `× slower` (медленнее) или `≈ same`
  (примерно одинаково), если медианы отличаются меньше чем на 5% или их
  диапазоны ±1 стандартное отклонение пересекаются.
- Каждый язык, бэкенд и размер выполняется в своей задаче GitHub Actions (на
  своей виртуальной машине), поэтому они не конкурируют за один процессор, и
  Redis работает в этой задаче как сервисный контейнер с отключёнными
  снимками. Запросы на слияние проверяют весь конвейер с `B = 1,000` и
  `N = 100`; отправки в `main` измеряют `B = 10,000`, `100,000` и `1,000,000`
  с `N = 1,000` и обновляют результаты. Workflow также можно запустить вручную
  с другими размерами.
- Строка происхождения каждой таблицы содержит версии Redis, клиента, Doublets
  и компилятора или SDK, процессор, коммит, дату и запуск.

### Ограничения

- Время Redis включает сериализацию в клиенте и локальный сетевой обмен, а
  Doublets работает в процессе бенчмарка; именно эту разницу между двумя
  путями доступа и измеряет сравнение. Удалённые сети, конкурирующие клиенты,
  расход памяти и сохранение на диск не измеряются.
- Бенчмарки используют один отдельный сервер Redis: функции вычисляют свои
  ключи по связи, поэтому они не работают в Redis Cluster.
- Бенчмарки и тесты очищают базу данных Redis из `REDIS_URL` (`FLUSHDB`),
  поэтому она должна быть одноразовой.
- Хранилища doublets от 0.5.0 до 0.8.0 (Rust) отказывают после 1,040,384 связей, потому
  что берут ту часть памяти, которую возвращает `platform-mem` после её роста;
  бенчмарки оборачивают память в [`memory::Whole`](rust/src/memory.rs),
  который возвращает всю память, а
  [`experiments/check-full-size.sh`](experiments/check-full-size.sh) проверяет
  наибольший запланированный размер.

## Локальный запуск

Запустите одноразовый сервер Redis без сохранения на диск, затем выполните
оба набора обоих языков и создайте таблицы и графики:

```sh
docker run -d --rm --name redis-comparison -p 127.0.0.1:6379:6379 \
  redis:8.10.2 redis-server --save '' --appendonly no
export REDIS_URL=redis://localhost:6379
python3 -m pip install -r scripts/requirements.txt
examples/run-comparison.sh 1000
```

Размеры задаются `BENCHMARK_BACKGROUND_LINKS` (`B`, по умолчанию 1,000) и
`BENCHMARK_LINKS` (`N`, по умолчанию 100), а `BENCHMARK_BACKEND=redis` или
`doublets` запускает только один бэкенд:

```sh
(cd rust && BENCHMARK_BACKEND=doublets cargo bench --bench bench)
BENCHMARK_BACKEND=redis dotnet run -c Release --project csharp/RedisVSDoublets
```

Тесты проверяют, что каждое хранилище даёт те же результаты, что и модель
операций, включая Redis в каждом режиме, и что каждая отмена восстанавливает
фоновые данные; все локальные проверки описаны в
[CONTRIBUTING.md](CONTRIBUTING.md).

## Результаты

Таблицы и графики создаёт [workflow Benchmarks](.github/workflows/benchmarks.yml)
при каждой отправке в `main`; они находятся в
[английской версии](README.md#results).
Исходные результаты проверочного запуска запроса на слияние, который добавил
сравнение (`B = 1,000`, `N = 100`), находятся в
[`docs/sample-results`](docs/sample-results).
