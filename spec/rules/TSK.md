# TSK: Tasks and tzroutines

The rules of this area, one sentence each, with the document that states them and the conformance tests that check them (`tests/conformance/TSK/`). See `spec/README.md` for the format.

| ID | Rule | Doc | Tests | Status |
| --- | --- | --- | --- | --- |
| TSK-001 | `Task.run` MUST run a task to completion and return its result. | - | - | retired |
| TSK-002 | A `Task` MUST NOT be Copy, so running it twice MUST be rejected (E1012). | - | - | retired |
| TSK-003 | A reference MUST NOT be sent into a task from outside (E1013). | - | - | retired |
| TSK-004 | `Task.parallel` MUST return its results in the order of the input array. | - | - | retired |
| TSK-005 | A task that is dropped without running MUST NOT execute its body. | - | - | retired |
| TSK-006 | An owned value MAY be moved into a task. | - | - | retired |
| TSK-007 | An empty `Task.parallel` MUST yield an empty result array. | - | - | retired |
| TSK-008 | Tasks of `Task.scope` that add to a shared `Atomic` with `Atomic.fetch_add` MUST produce the same total on every run. | - | - | retired |
| TSK-009 | A `Task.parallel_results` that contains an `Error` MUST return the `Error` of the smallest input index. | - | - | retired |
| TSK-010 | A `Task<T>` value MUST NOT be used where a `T` is expected (E1003). | - | - | retired |
| TSK-011 | `tzroutine { }` MUST return before its body finishes, and the body MUST run exactly once. | [language.md § tzroutine](../../docs/language.md#tzroutine) | TSK-011-tzroutine-must-return-before | active |
| TSK-012 | A boundary (`main`, an export call, a test or a bench sample) MUST NOT return before every tzroutine spawned in it has finished. | [language.md § tzroutine](../../docs/language.md#tzroutine) | TSK-012-boundary-must-not-return | active |
| TSK-013 | A tzroutine MUST NOT capture a borrow (E1013). | [language.md § tzroutine](../../docs/language.md#tzroutine) | TSK-013-tzroutine-must-not-capture | active |
| TSK-014 | The body of a tzroutine MUST have type `unit` (E1003). | [language.md § tzroutine](../../docs/language.md#tzroutine) | TSK-014-body-of-tzroutine-must | active |
| TSK-015 | `let!` and `do!` in a tzroutine body MUST run the IO action at that point. | [language.md § tzroutine](../../docs/language.md#tzroutine) | TSK-015-let-and-do-in | active |
| TSK-016 | `Channel.recv` MUST return `None` only after the channel is closed and every item sent before the close has been received. | [language.md § Channel](../../docs/language.md#channel) | TSK-016-channel-recv-must-return | active |
| TSK-017 | A `send` on a closed `Channel` MUST trap. | [language.md § Channel](../../docs/language.md#channel) | TSK-017-send-on-closed-channel | active |
| TSK-018 | Items of one `Channel` MUST be received in the order their sends completed. | [language.md § Channel](../../docs/language.md#channel) | TSK-018-items-of-one-channel | active |
| TSK-019 | `for x in ch` MUST end when `ch` is closed and empty. | [language.md § Channel](../../docs/language.md#channel) | TSK-019-for-x-in-ch | active |
| TSK-020 | A `select` MUST complete exactly one ready arm, or run `default` when none is ready. | [language.md § select](../../docs/language.md#select) | TSK-020-select-must-complete-exactly | active |
| TSK-021 | An arm whose `when` guard is false MUST NOT be chosen, and its channel and value MUST NOT be evaluated. | [language.md § select](../../docs/language.md#select) | TSK-021-arm-whose-when-guard | active |
| TSK-022 | A send value of an arm that is not chosen MUST be dropped exactly once. | [language.md § select](../../docs/language.md#select) | TSK-022-send-value-of-arm | active |
| TSK-023 | `Time.sleep` in a tzroutine MUST NOT stop other tzroutines. | [language.md § OS API](../../docs/language.md#os-api) | TSK-023-time-sleep-in-tzroutine | active |
| TSK-024 | A stopped `Timer` MUST NOT send. | [language.md § タイマー](../../docs/language.md#タイマー) | TSK-024-stopped-timer-must-not | active |
| TSK-025 | A boundary MUST NOT wait for pending timers, and MUST release them. | [language.md § OS API](../../docs/language.md#os-api) | TSK-025-boundary-must-not-wait | active |
| TSK-026 | `WaitGroup.wait` MUST return only after the counter is zero. | [language.md § 同期の型（TZ06）](../../docs/language.md#同期の型tz06) | TSK-026-waitgroup-wait-must-return | active |
| TSK-027 | `Once.call` MUST run its function at most once. | [language.md § 同期の型（TZ06）](../../docs/language.md#同期の型tz06) | TSK-027-once-call-must-run | active |
| TSK-028 | A tzroutine MAY wait while it holds a `Mutex`. | [language.md § Atomic / Mutex](../../docs/language.md#atomic--mutex) | TSK-028-tzroutine-may-wait-while | active |
| TSK-029 | Canceling a `Context` MUST close the `done` channel of it and of every descendant. | [language.md § 取り消しと ErrGroup（TZ07）](../../docs/language.md#取り消しと-errgrouptz07) | TSK-029-canceling-a-context-must | active |
| TSK-030 | `ErrGroup.wait` MUST return only after every function given to `go` has returned. | [language.md § 取り消しと ErrGroup（TZ07）](../../docs/language.md#取り消しと-errgrouptz07) | TSK-030-errgroup-wait-must-return | active |
| TSK-031 | A tzroutine blocked in an `extern` call MUST NOT stop the other tzroutines from running. | [language.md § tzroutine](../../docs/language.md#tzroutine) | TSK-031-tzroutine-blocked-in-extern | active |
