# TSK: Tasks and tzroutines

The rules of this area, one sentence each, with the document that states them and the conformance tests that check them (`tests/conformance/TSK/`). See `spec/README.md` for the format.

| ID | Rule | Doc | Tests | Status |
| --- | --- | --- | --- | --- |
| TSK-001 | `Task.run` MUST run a task to completion and return its result. | [tzroutine.md](../../_tsuzuri/language-reference/async-tasks-and-lazy/tzroutine.md) | TSK-001-task-run-must-run | retired |
| TSK-002 | A `Task` MUST NOT be Copy, so running it twice MUST be rejected (E1012). | [tzroutine.md](../../_tsuzuri/language-reference/async-tasks-and-lazy/tzroutine.md) | TSK-002-task-must-not-be | retired |
| TSK-003 | A reference MUST NOT be sent into a task from outside (E1013). | [tzroutine.md](../../_tsuzuri/language-reference/async-tasks-and-lazy/tzroutine.md) | TSK-003-reference-must-not-be | retired |
| TSK-004 | `Task.parallel` MUST return its results in the order of the input array. | [tzroutine.md](../../_tsuzuri/language-reference/async-tasks-and-lazy/tzroutine.md) | TSK-004-task-parallel-must-return | retired |
| TSK-005 | A task that is dropped without running MUST NOT execute its body. | [tzroutine.md](../../_tsuzuri/language-reference/async-tasks-and-lazy/tzroutine.md) | TSK-005-task-that-is-dropped | retired |
| TSK-006 | An owned value MAY be moved into a task. | [tzroutine.md](../../_tsuzuri/language-reference/async-tasks-and-lazy/tzroutine.md) | TSK-006-owned-value-may-be | retired |
| TSK-007 | An empty `Task.parallel` MUST yield an empty result array. | [tzroutine.md](../../_tsuzuri/language-reference/async-tasks-and-lazy/tzroutine.md) | TSK-007-empty-task-parallel-must | retired |
| TSK-008 | Tasks of `Task.scope` that add to a shared `Atomic` with `Atomic.fetch_add` MUST produce the same total on every run. | [tzroutine.md](../../_tsuzuri/language-reference/async-tasks-and-lazy/tzroutine.md) | TSK-008-tasks-sharing-atomic-through | retired |
| TSK-009 | A `Task.parallel_results` that contains an `Error` MUST return the `Error` of the smallest input index. | [tzroutine.md](../../_tsuzuri/language-reference/async-tasks-and-lazy/tzroutine.md) | TSK-009-task-parallel-results-that | retired |
| TSK-010 | A `Task<T>` value MUST NOT be used where a `T` is expected (E1003). | [tzroutine.md](../../_tsuzuri/language-reference/async-tasks-and-lazy/tzroutine.md) | TSK-010-task-t-value-must | retired |
| TSK-011 | `tzroutine { }` MUST return before its body finishes, and the body MUST run exactly once. | [tzroutine.md](../../_tsuzuri/language-reference/async-tasks-and-lazy/tzroutine.md) | TSK-011-tzroutine-must-return-before | active |
| TSK-012 | A boundary (`main`, an export call, a test or a bench sample) MUST NOT return before every tzroutine spawned in it has finished. | [tzroutine.md](../../_tsuzuri/language-reference/async-tasks-and-lazy/tzroutine.md) | TSK-012-boundary-must-not-return | active |
| TSK-013 | A tzroutine MUST NOT capture a borrow (E1013). | [tzroutine.md](../../_tsuzuri/language-reference/async-tasks-and-lazy/tzroutine.md) | TSK-013-tzroutine-must-not-capture | active |
| TSK-014 | The body of a tzroutine MUST have type `unit` (E1003). | [tzroutine.md](../../_tsuzuri/language-reference/async-tasks-and-lazy/tzroutine.md) | TSK-014-body-of-tzroutine-must | active |
| TSK-015 | `let!` and `do!` in a tzroutine body MUST run the IO action at that point. | [tzroutine.md](../../_tsuzuri/language-reference/async-tasks-and-lazy/tzroutine.md) | TSK-015-let-and-do-in | active |
| TSK-016 | `Channel.recv` MUST return `None` only after the channel is closed and every item sent before the close has been received. | [channel.md](../../_tsuzuri/language-reference/built-in-types-and-modules/channel.md) | TSK-016-channel-recv-must-return | active |
| TSK-017 | A `send` on a closed `Channel` MUST trap. | [channel.md](../../_tsuzuri/language-reference/built-in-types-and-modules/channel.md) | TSK-017-send-on-closed-channel | active |
| TSK-018 | Items of one `Channel` MUST be received in the order their sends completed. | [channel.md](../../_tsuzuri/language-reference/built-in-types-and-modules/channel.md) | TSK-018-items-of-one-channel | active |
| TSK-019 | `for x in ch` MUST end when `ch` is closed and empty. | [channel.md](../../_tsuzuri/language-reference/built-in-types-and-modules/channel.md) | TSK-019-for-x-in-ch | active |
| TSK-020 | A `select` MUST complete exactly one ready arm, or run `default` when none is ready. | [select.md](../../_tsuzuri/language-reference/async-tasks-and-lazy/select.md) | TSK-020-select-must-complete-exactly | active |
| TSK-021 | An arm whose `when` guard is false MUST NOT be chosen, and its channel and value MUST NOT be evaluated. | [select.md](../../_tsuzuri/language-reference/async-tasks-and-lazy/select.md) | TSK-021-arm-whose-when-guard | active |
| TSK-022 | A send value of an arm that is not chosen MUST be dropped exactly once. | [select.md](../../_tsuzuri/language-reference/async-tasks-and-lazy/select.md) | TSK-022-send-value-of-arm | active |
| TSK-023 | `Time.sleep` in a tzroutine MUST NOT stop other tzroutines. | [time.md](../../_tsuzuri/language-reference/built-in-types-and-modules/time.md) | TSK-023-time-sleep-in-tzroutine | active |
| TSK-024 | A stopped `Timer` MUST NOT send. | [timer.md](../../_tsuzuri/language-reference/built-in-types-and-modules/timer.md) | TSK-024-stopped-timer-must-not | active |
| TSK-025 | A boundary MUST NOT wait for pending timers, and MUST release them. | [time.md](../../_tsuzuri/language-reference/built-in-types-and-modules/time.md) | TSK-025-boundary-must-not-wait | active |
| TSK-026 | `WaitGroup.wait` MUST return only after the counter is zero. | [waitgroup.md](../../_tsuzuri/language-reference/built-in-types-and-modules/waitgroup.md) | TSK-026-waitgroup-wait-must-return | active |
| TSK-027 | `Once.call` MUST run its function at most once. | [once.md](../../_tsuzuri/language-reference/built-in-types-and-modules/once.md) | TSK-027-once-call-must-run | active |
| TSK-028 | A tzroutine MAY wait while it holds a `Mutex`. | [mutex.md](../../_tsuzuri/language-reference/built-in-types-and-modules/mutex.md) | TSK-028-tzroutine-may-wait-while | active |
| TSK-029 | Canceling a `Context` MUST close the `done` channel of it and of every descendant. | [context.md](../../_tsuzuri/language-reference/built-in-types-and-modules/context.md) | TSK-029-canceling-a-context-must | active |
| TSK-030 | `ErrGroup.wait` MUST return only after every function given to `go` has returned. | [errgroup.md](../../_tsuzuri/language-reference/built-in-types-and-modules/errgroup.md) | TSK-030-errgroup-wait-must-return | active |
| TSK-031 | A tzroutine blocked in an `extern` call MUST NOT stop the other tzroutines from running. | [tzroutine.md](../../_tsuzuri/language-reference/async-tasks-and-lazy/tzroutine.md) | TSK-031-tzroutine-blocked-in-extern | active |
