# TSK: Tasks

The rules of this area, one sentence each, with the document that states them and the conformance tests that check them (`tests/conformance/TSK/`). See `spec/README.md` for the format.

| ID | Rule | Doc | Tests | Status |
| --- | --- | --- | --- | --- |
| TSK-001 | `Task.run` MUST run a task to completion and return its result. | [task.md](../../_tsuzuri/language-reference/async-tasks-and-lazy/task.md) | TSK-001-task-run-must-run | active |
| TSK-002 | A `Task` MUST NOT be Copy, so running it twice MUST be rejected (E1012). | [task.md](../../_tsuzuri/language-reference/async-tasks-and-lazy/task.md) | TSK-002-task-must-not-be | active |
| TSK-003 | A reference MUST NOT be sent into a task from outside (E1013). | [task.md](../../_tsuzuri/language-reference/async-tasks-and-lazy/task.md) | TSK-003-reference-must-not-be | active |
| TSK-004 | `Task.parallel` MUST return its results in the order of the input array. | [task.md](../../_tsuzuri/language-reference/async-tasks-and-lazy/task.md) | TSK-004-task-parallel-must-return | active |
| TSK-005 | A task that is dropped without running MUST NOT execute its body. | [task.md](../../_tsuzuri/language-reference/async-tasks-and-lazy/task.md) | TSK-005-task-that-is-dropped | active |
| TSK-006 | An owned value MAY be moved into a task. | [task.md](../../_tsuzuri/language-reference/async-tasks-and-lazy/task.md) | TSK-006-owned-value-may-be | active |
| TSK-007 | An empty `Task.parallel` MUST yield an empty result array. | [task.md](../../_tsuzuri/language-reference/async-tasks-and-lazy/task.md) | TSK-007-empty-task-parallel-must | active |
| TSK-008 | Tasks sharing an `Atomic` through `Task.scope` MUST produce the same total on every run. | [task.md](../../_tsuzuri/language-reference/async-tasks-and-lazy/task.md) | TSK-008-tasks-sharing-atomic-through | active |
| TSK-009 | A `Task.parallel_results` that contains an `Error` MUST return the `Error` of the smallest input index. | [task.md](../../_tsuzuri/language-reference/async-tasks-and-lazy/task.md) | TSK-009-task-parallel-results-that | active |
| TSK-010 | A `Task<T>` value MUST NOT be used where a `T` is expected (E1003). | [task.md](../../_tsuzuri/language-reference/async-tasks-and-lazy/task.md) | TSK-010-task-t-value-must | active |
