//! Deterministic data parallelism for the frontend (PB04).
//!
//! A parallel stage runs a function over the items `0..count` on scoped worker threads and hands
//! the results to the caller one at a time, in item order. The caller's consumer is the loop
//! that the sequential code ran, so every accumulator (diagnostics with their cap, warnings, the
//! typed functions) sees the same events in the same order whatever the number of threads.
//! With one thread, or too little work, the stage runs on the calling thread and evaluates an
//! item only when the consumer asks for it, as the sequential code always did.
//!
//! The workers never touch the timings of `TSUZURI_TIME_PASSES`: a stage is measured on the
//! calling thread, so a phase is wall-clock time and the phases still add up to at most the total.

use crate::diagnostic::{Diagnostic, DiagnosticKind, Span};
use std::ffi::OsStr;
use std::ops::ControlFlow;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};

/// The environment variable that bounds the threads the compiler itself uses.
pub(crate) const THREADS_VARIABLE: &str = "TSUZURI_THREADS";
/// The largest value that `TSUZURI_THREADS` accepts.
pub(crate) const MAX_THREADS: usize = 32;
/// The most threads that automatic selection uses.
pub(crate) const AUTOMATIC_THREADS: usize = 8;
/// The stack of a worker, what the main thread has by default on macOS and Linux (and eight times
/// what Windows gives it): a worker must not overflow on a program that the main thread accepts.
pub(crate) const WORKER_STACK_BYTES: usize = 8 << 20;

/// How many threads the frontend may use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Parallelism {
    threads: usize,
    /// Tests only: every stage uses all the threads, however little work it has, so that small
    /// programs exercise the parallel paths too.
    #[cfg(test)]
    eager: bool,
}

impl Parallelism {
    /// One thread: every stage runs on the calling thread, as it did before PB04.
    pub(crate) const SEQUENTIAL: Self = Self {
        threads: 1,
        #[cfg(test)]
        eager: false,
    };

    /// At most `threads` workers, between 1 and `MAX_THREADS`.
    pub(crate) fn fixed(threads: usize) -> Self {
        Self {
            threads: threads.clamp(1, MAX_THREADS),
            #[cfg(test)]
            eager: false,
        }
    }

    /// `fixed(threads)`, with every stage that has two items or more on that many workers.
    #[cfg(test)]
    pub(crate) fn eager(threads: usize) -> Self {
        Self {
            eager: true,
            ..Self::fixed(threads)
        }
    }

    /// The available cores, at most `AUTOMATIC_THREADS`.
    pub(crate) fn automatic() -> Self {
        let available = std::thread::available_parallelism().map_or(1, usize::from);
        Self::fixed(available.min(AUTOMATIC_THREADS))
    }

    /// What the library entry points use: one thread, unless `TSUZURI_THREADS` says otherwise.
    pub(crate) fn library() -> Self {
        Self::from_environment(Self::SEQUENTIAL)
    }

    /// What the commands of the command line use: the available cores (at most
    /// `AUTOMATIC_THREADS`), unless `TSUZURI_THREADS` says otherwise.
    pub(crate) fn command_line() -> Self {
        Self::from_environment(Self::automatic())
    }

    /// `default` when `TSUZURI_THREADS` is unset, empty or invalid. The command line rejects an
    /// invalid value before it analyzes anything (`validate_environment`).
    fn from_environment(default: Self) -> Self {
        Self::from_setting(std::env::var_os(THREADS_VARIABLE).as_deref(), default)
            .unwrap_or(default)
    }

    /// Reports an invalid `TSUZURI_THREADS`.
    pub(crate) fn validate_environment() -> Result<(), Diagnostic> {
        thread_setting(std::env::var_os(THREADS_VARIABLE).as_deref()).map(|_| ())
    }

    fn from_setting(value: Option<&OsStr>, default: Self) -> Result<Self, Diagnostic> {
        Ok(thread_setting(value)?.map_or(default, Self::fixed))
    }

    /// How many workers `units` of work justify: one per `per_worker` units, and at most the
    /// threads allowed. Fewer than 2 means that the stage runs on the calling thread.
    pub(crate) fn workers(self, units: usize, per_worker: usize) -> usize {
        #[cfg(test)]
        if self.eager {
            return self.threads.min(units);
        }
        self.threads.min(units / per_worker.max(1))
    }
}

/// The meaning of `TSUZURI_THREADS`: `None` for unset or empty, which leaves the choice to the
/// caller, and an integer from 1 to `MAX_THREADS` (surrounding white space allowed) otherwise.
pub(crate) fn thread_setting(value: Option<&OsStr>) -> Result<Option<usize>, Diagnostic> {
    let Some(value) = value.filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    value
        .to_str()
        .map(str::trim)
        .filter(|text| !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit()))
        .and_then(|text| text.parse::<usize>().ok())
        .filter(|threads| (1..=MAX_THREADS).contains(threads))
        .map(Some)
        .ok_or_else(|| {
            Diagnostic::of(DiagnosticKind::InvalidThreadSetting,
                format!(
                    "{THREADS_VARIABLE} must be an integer from 1 to {MAX_THREADS}; unset it to use the available cores"
                ),
                Span::default(),
            )
        })
}

#[cfg(test)]
thread_local! {
    /// How many stages started workers on this thread: tests use it to see that a stage really ran
    /// in parallel (or, for a small input, did not).
    pub(crate) static PARALLEL_STAGES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// The results of the chunks that the workers have finished, and the chunk that the consumer waits for.
struct Board<T> {
    chunks: Vec<Option<Vec<std::thread::Result<T>>>>,
    waiting_for: Option<usize>,
}

struct Shared<T> {
    /// The next position in the claim order.
    claimed: AtomicUsize,
    /// Set when the consumer needs no more results.
    stop: AtomicBool,
    board: Mutex<Board<T>>,
    ready: Condvar,
}

impl<T> Shared<T> {
    fn board(&self) -> MutexGuard<'_, Board<T>> {
        // The code that holds the lock does not panic, so the board is never half written.
        self.board.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn publish(&self, chunk: usize, results: Vec<std::thread::Result<T>>) {
        let mut board = self.board();
        board.chunks[chunk] = Some(results);
        if board.waiting_for == Some(chunk) {
            self.ready.notify_one();
        }
    }

    fn take(&self, chunk: usize) -> Vec<std::thread::Result<T>> {
        let mut board = self.board();
        loop {
            if let Some(results) = board.chunks[chunk].take() {
                board.waiting_for = None;
                return results;
            }
            board.waiting_for = Some(chunk);
            board = self
                .ready
                .wait(board)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }
}

/// Tells the workers to stop when the consumer leaves the stage by any path, a panic included.
struct StopOnExit<'a>(&'a AtomicBool);

impl Drop for StopOnExit<'_> {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

/// The result of one item, which the consumer may leave unread. An item that the consumer never
/// reads is never evaluated by a stage on one thread, so a stage that stops at its first full
/// `Diagnostics` does exactly the work of the loop that it replaces. A panic of the worker that
/// evaluated the item resumes in `get`, with its payload.
pub(crate) struct Item<'a, T>(Inner<'a, T>);

enum Inner<'a, T> {
    Done(std::thread::Result<T>),
    Deferred(usize, &'a dyn Fn(usize) -> T),
}

impl<T> Item<'_, T> {
    pub(crate) fn get(self) -> T {
        match self.0 {
            Inner::Done(Ok(value)) => value,
            Inner::Done(Err(payload)) => resume_unwind(payload),
            Inner::Deferred(item, produce) => produce(item),
        }
    }
}

/// Calls `consume(item, result)` for the items `0..count` in item order until it breaks. The
/// result is `produce(item)`, evaluated on a worker thread before the consumer reaches the item,
/// or, on a stage with fewer than 2 `workers`, by the consumer's `get` on the calling thread.
///
/// With more workers, they take `chunk` consecutive items at a time. They claim the chunks in the
/// order of `claim`, a permutation of the chunk numbers (chunk `c` holds the items
/// `c * chunk..`), or in order. The calling thread does not evaluate items, so every item runs on
/// a worker with the same explicit stack. When `consume` breaks, workers may already have
/// evaluated items beyond the last one consumed, and those results are dropped. If the system
/// refuses every thread, the calling thread runs the stage alone.
pub(crate) fn ordered_for_each<T, P, C>(
    count: usize,
    workers: usize,
    chunk: usize,
    claim: Option<&[usize]>,
    produce: P,
    consume: C,
) where
    T: Send,
    P: Fn(usize) -> T + Sync,
    C: FnMut(usize, Item<'_, T>) -> ControlFlow<()>,
{
    ordered_for_each_with_stack(
        WORKER_STACK_BYTES,
        count,
        workers,
        chunk,
        claim,
        produce,
        consume,
    );
}

/// `ordered_for_each` whose workers have `stack` bytes of stack: a stack that the system refuses
/// leaves the stage to the calling thread, which the tests use.
fn ordered_for_each_with_stack<T, P, C>(
    stack: usize,
    count: usize,
    workers: usize,
    chunk: usize,
    claim: Option<&[usize]>,
    produce: P,
    mut consume: C,
) where
    T: Send,
    P: Fn(usize) -> T + Sync,
    C: FnMut(usize, Item<'_, T>) -> ControlFlow<()>,
{
    let sequential = |consume: &mut C| {
        for item in 0..count {
            if consume(item, Item(Inner::Deferred(item, &produce))).is_break() {
                break;
            }
        }
    };
    if workers < 2 || count < 2 {
        return sequential(&mut consume);
    }
    let chunk = chunk.max(1);
    let chunks = count.div_ceil(chunk);
    debug_assert!(claim.is_none_or(|claim| claim.len() == chunks));
    let shared = Shared {
        claimed: AtomicUsize::new(0),
        stop: AtomicBool::new(false),
        board: Mutex::new(Board {
            chunks: (0..chunks).map(|_| None).collect(),
            waiting_for: None,
        }),
        ready: Condvar::new(),
    };
    let work = || {
        while !shared.stop.load(Ordering::Relaxed) {
            let position = shared.claimed.fetch_add(1, Ordering::Relaxed);
            if position >= chunks {
                break;
            }
            let number = claim.map_or(position, |claim| claim[position]);
            let items = number * chunk..((number + 1) * chunk).min(count);
            let mut results = Vec::with_capacity(items.len());
            for item in items {
                results.push(catch_unwind(AssertUnwindSafe(|| produce(item))));
            }
            shared.publish(number, results);
        }
    };
    std::thread::scope(|scope| {
        let mut started = 0;
        while started < workers.min(chunks)
            && std::thread::Builder::new()
                .name("tsuzuri-frontend".into())
                .stack_size(stack)
                .spawn_scoped(scope, work)
                .is_ok()
        {
            started += 1;
        }
        if started == 0 {
            return sequential(&mut consume);
        }
        #[cfg(test)]
        PARALLEL_STAGES.with(|stages| stages.set(stages.get() + 1));
        let _stop = StopOnExit(&shared.stop);
        let mut item = 0;
        for number in 0..chunks {
            for result in shared.take(number) {
                if consume(item, Item(Inner::Done(result))).is_break() {
                    return;
                }
                item += 1;
            }
        }
    });
}

/// `ordered_for_each` over `items` that each worker takes by value, such as the `&mut` borrows
/// of the elements of a vector: `produce(index, item)` runs once for each item.
pub(crate) fn ordered_for_each_owned<I, T, P, C>(
    items: impl IntoIterator<Item = I>,
    workers: usize,
    chunk: usize,
    claim: Option<&[usize]>,
    produce: P,
    consume: C,
) where
    I: Send,
    T: Send,
    P: Fn(usize, I) -> T + Sync,
    C: FnMut(usize, Item<'_, T>) -> ControlFlow<()>,
{
    let slots: Vec<Mutex<Option<I>>> = items
        .into_iter()
        .map(|item| Mutex::new(Some(item)))
        .collect();
    ordered_for_each(
        slots.len(),
        workers,
        chunk,
        claim,
        |index| {
            let item = slots[index]
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .take()
                .expect("a stage evaluates each item once");
            produce(index, item)
        },
        consume,
    );
}

/// The message of a panic payload that `panic!` made, for tests.
#[cfg(test)]
fn message(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| {
            payload
                .downcast_ref::<&str>()
                .map(|text| (*text).to_owned())
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    fn setting(text: &str) -> Result<Option<usize>, Diagnostic> {
        thread_setting(Some(OsString::from(text).as_os_str()))
    }

    #[test]
    fn thread_setting_accepts_1_to_32_and_leaves_unset_to_the_caller() {
        assert_eq!(thread_setting(None).unwrap(), None);
        assert_eq!(setting("").unwrap(), None);
        for (text, expected) in [("1", 1), (" 8 ", 8), ("32", 32), ("08", 8), ("\t4\n", 4)] {
            assert_eq!(setting(text).unwrap(), Some(expected), "{text:?}");
        }
        for text in [
            "0",
            "33",
            "-1",
            "+4",
            "abc",
            "1.5",
            " ",
            "1 2",
            "0x10",
            "1_0",
            "４",
            "99999999999999999999",
        ] {
            let error = setting(text).unwrap_err();
            assert_eq!(error.code, "E2000", "{text:?}");
            assert_eq!(
                error.message,
                "TSUZURI_THREADS must be an integer from 1 to 32; unset it to use the available cores"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn thread_setting_rejects_a_value_that_is_not_utf8() {
        use std::os::unix::ffi::OsStrExt;
        let error = thread_setting(Some(OsStr::from_bytes(b"4\xff"))).unwrap_err();
        assert_eq!(error.code, "E2000");
    }

    #[test]
    fn parallelism_follows_the_setting_and_the_default() {
        let eight = Parallelism::fixed(8);
        let value = |text: &str| Some(OsString::from(text));
        assert_eq!(Parallelism::from_setting(None, eight).unwrap(), eight);
        assert_eq!(
            Parallelism::from_setting(value("").as_deref(), eight).unwrap(),
            eight
        );
        assert_eq!(
            Parallelism::from_setting(value("1").as_deref(), eight).unwrap(),
            Parallelism::SEQUENTIAL
        );
        assert_eq!(
            Parallelism::from_setting(value("32").as_deref(), Parallelism::SEQUENTIAL)
                .unwrap()
                .threads,
            32
        );
        assert!(Parallelism::from_setting(value("0").as_deref(), eight).is_err());
        assert_eq!(Parallelism::fixed(0), Parallelism::SEQUENTIAL);
        assert_eq!(Parallelism::fixed(1000).threads, MAX_THREADS);
        let automatic = Parallelism::automatic().threads;
        assert!((1..=AUTOMATIC_THREADS).contains(&automatic));
    }

    #[test]
    fn a_stack_that_the_system_refuses_leaves_the_stage_to_the_calling_thread() {
        let caller = std::thread::current().id();
        let mut seen = Vec::new();
        // No thread can have a stack this large, so none starts: the stage runs where it is called.
        ordered_for_each_with_stack(
            usize::MAX / 2,
            50,
            4,
            3,
            None,
            |item| (item, std::thread::current().id()),
            |index, result| {
                let (item, thread) = result.get();
                assert_eq!((index, thread), (item, caller));
                seen.push(item);
                ControlFlow::Continue(())
            },
        );
        assert_eq!(seen, (0..50).collect::<Vec<_>>());
        assert_eq!(PARALLEL_STAGES.get(), 0, "no worker started");
    }

    #[test]
    fn eager_parallelism_ignores_the_amount_of_work() {
        let eager = Parallelism::eager(8);
        assert_eq!(eager.workers(0, 256), 0);
        assert_eq!(eager.workers(3, 256), 3);
        assert_eq!(eager.workers(1_000_000, 256), 8);
        assert_eq!(Parallelism::eager(1).workers(100, 4), 1);
        assert_ne!(eager, Parallelism::fixed(8));
        assert_eq!(Parallelism::fixed(8).workers(100, 256), 0);
    }

    #[test]
    fn workers_follow_the_amount_of_work() {
        let eight = Parallelism::fixed(8);
        assert_eq!(eight.workers(0, 256), 0);
        assert_eq!(eight.workers(255, 256), 0);
        assert_eq!(eight.workers(256, 256), 1);
        assert_eq!(eight.workers(512, 256), 2);
        assert_eq!(eight.workers(1300, 256), 5);
        assert_eq!(eight.workers(1_000_000, 256), 8);
        assert_eq!(Parallelism::SEQUENTIAL.workers(1_000_000, 256), 1);
        assert_eq!(eight.workers(10, 0), 8);
    }

    fn collect(
        count: usize,
        workers: usize,
        chunk: usize,
        claim: Option<&[usize]>,
    ) -> Vec<(usize, usize)> {
        let mut seen = Vec::new();
        ordered_for_each(
            count,
            workers,
            chunk,
            claim,
            |item| item * item,
            |item, value| {
                seen.push((item, value.get()));
                ControlFlow::Continue(())
            },
        );
        seen
    }

    #[test]
    fn results_arrive_in_item_order_for_every_shape() {
        for count in [0, 1, 2, 7, 8, 9, 100] {
            let expected: Vec<_> = (0..count).map(|item| (item, item * item)).collect();
            for workers in [0, 1, 2, 3, 8] {
                for chunk in [0, 1, 3, 8] {
                    assert_eq!(
                        collect(count, workers, chunk, None),
                        expected,
                        "{count} {workers} {chunk}"
                    );
                    let reversed: Vec<_> = (0..count.div_ceil(chunk.max(1))).rev().collect();
                    assert_eq!(
                        collect(count, workers, chunk, Some(reversed.as_slice())),
                        expected,
                        "reversed {count} {workers} {chunk}"
                    );
                }
            }
        }
    }

    #[test]
    fn the_consumer_stops_the_stage_early() {
        for (workers, chunk) in [(0, 4), (1, 4), (2, 4), (4, 1), (8, 8)] {
            let produced = AtomicUsize::new(0);
            let mut consumed = Vec::new();
            ordered_for_each(
                10_000,
                workers,
                chunk,
                None,
                |item| {
                    produced.fetch_add(1, Ordering::Relaxed);
                    item
                },
                |item, value| {
                    consumed.push((item, value.get()));
                    if item == 20 {
                        ControlFlow::Break(())
                    } else {
                        ControlFlow::Continue(())
                    }
                },
            );
            assert_eq!(
                consumed,
                (0..=20).map(|item| (item, item)).collect::<Vec<_>>()
            );
            let produced = produced.into_inner();
            if workers < 2 {
                assert_eq!(
                    produced, 21,
                    "the sequential stage evaluates an item when it is read"
                );
            } else {
                // Workers run ahead of the consumer, so only the lower bound is certain.
                assert!(produced >= 21, "{workers}: {produced}");
            }
        }
    }

    #[test]
    fn an_item_that_the_consumer_does_not_read_is_not_evaluated_on_one_thread() {
        let produced = AtomicUsize::new(0);
        let mut reached = Vec::new();
        ordered_for_each(
            100,
            1,
            8,
            None,
            |item| {
                produced.fetch_add(1, Ordering::Relaxed);
                item
            },
            |item, value| {
                reached.push(item);
                if item == 5 {
                    // As a full `Diagnostics` stops a loop before it checks the next function.
                    return ControlFlow::Break(());
                }
                value.get();
                ControlFlow::Continue(())
            },
        );
        assert_eq!(reached, (0..=5).collect::<Vec<_>>());
        assert_eq!(produced.into_inner(), 5);
    }

    #[test]
    fn a_worker_panic_reaches_the_caller_at_its_item_with_its_payload() {
        for workers in [2, 4, 8] {
            let mut consumed = Vec::new();
            let outcome = catch_unwind(AssertUnwindSafe(|| {
                ordered_for_each(
                    64,
                    workers,
                    2,
                    None,
                    |item| {
                        if item == 9 || item == 40 {
                            panic!("item {item} failed");
                        }
                        item
                    },
                    |item, value| {
                        value.get();
                        consumed.push(item);
                        ControlFlow::Continue(())
                    },
                );
            }));
            let payload = outcome.unwrap_err();
            assert_eq!(message(&*payload), "item 9 failed", "{workers} workers");
            assert_eq!(consumed, (0..9).collect::<Vec<_>>());
        }
    }

    #[test]
    fn a_panic_in_an_item_that_the_consumer_never_reads_is_ignored() {
        let mut consumed = Vec::new();
        ordered_for_each(
            64,
            4,
            2,
            None,
            |item| {
                if item >= 20 {
                    panic!("item {item} failed");
                }
                item
            },
            |item, value| {
                if item == 20 {
                    return ControlFlow::Break(());
                }
                consumed.push(value.get());
                ControlFlow::Continue(())
            },
        );
        assert_eq!(consumed, (0..20).collect::<Vec<_>>());
    }

    #[test]
    fn a_consumer_panic_stops_the_workers_without_hanging() {
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            ordered_for_each(
                100_000,
                4,
                1,
                None,
                |item| item,
                |item, _| {
                    assert!(item < 3, "the consumer gives up");
                    ControlFlow::Continue(())
                },
            );
        }));
        assert!(outcome.is_err());
    }

    fn deep(depth: usize) -> usize {
        // One 4 KiB array in each frame that the optimizer cannot remove.
        let mut frame = [1u8; 4096];
        std::hint::black_box(&mut frame);
        if depth == 0 {
            usize::from(frame[0])
        } else {
            deep(depth - 1) + usize::from(frame[depth % 4096])
        }
    }

    #[test]
    fn workers_have_the_explicit_8_mib_stack() {
        // About 5 MiB deep: more than a test thread has by default (2 MiB, or 4 MiB in CI).
        // Every level adds the byte 1, so the sum is the depth plus the bottom level.
        let mut results = Vec::new();
        ordered_for_each(
            4,
            2,
            1,
            None,
            |_| deep(1200),
            |_, value| {
                results.push(value.get());
                ControlFlow::Continue(())
            },
        );
        assert_eq!(results, vec![1201; 4]);
    }

    #[test]
    fn owned_items_are_delivered_once_and_mutable_borrows_reach_the_workers() {
        let mut values: Vec<u64> = (0..50).collect();
        let mut doubled = Vec::new();
        ordered_for_each_owned(
            values.iter_mut(),
            3,
            4,
            None,
            |index, value: &mut u64| {
                *value *= 2;
                (index, *value)
            },
            |item, result| {
                let (index, value) = result.get();
                assert_eq!(item, index);
                doubled.push(value);
                ControlFlow::Continue(())
            },
        );
        assert_eq!(doubled, (0..50).map(|value| value * 2).collect::<Vec<_>>());
        assert_eq!(values, doubled);
        let names: Vec<String> = (0..20).map(|index| format!("item {index}")).collect();
        let mut lengths = Vec::new();
        ordered_for_each_owned(
            names,
            4,
            1,
            None,
            |_, name: String| name.len(),
            |_, length| {
                lengths.push(length.get());
                ControlFlow::Continue(())
            },
        );
        assert_eq!(
            lengths,
            (0..20)
                .map(|index| format!("item {index}").len())
                .collect::<Vec<_>>()
        );
    }
}
