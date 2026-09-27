#define WIN32_LEAN_AND_MEAN
#define _CRT_SECURE_NO_WARNINGS
#include <windows.h>
#include <assert.h>
#include <stdint.h>
#include <stdatomic.h>
#include <stdlib.h>
#include <string.h>

static const char *operation;

static HANDLE WINAPI test_create(LPSECURITY_ATTRIBUTES attributes, SIZE_T size, LPTHREAD_START_ROUTINE run, LPVOID context, DWORD flags, LPDWORD id) {
    if (strcmp(operation, "create") == 0) { SetLastError(ERROR_NOT_ENOUGH_MEMORY); return NULL; }
    return CreateThread(attributes, size, run, context, flags, id);
}

static DWORD WINAPI test_wait(HANDLE thread, DWORD timeout) {
    if (strcmp(operation, "wait") == 0) { SetLastError(ERROR_INVALID_HANDLE); return WAIT_FAILED; }
    return WaitForSingleObject(thread, timeout);
}

static BOOL WINAPI test_close(HANDLE thread) {
    if (strcmp(operation, "close") == 0) { SetLastError(ERROR_INVALID_HANDLE); return FALSE; }
    return CloseHandle(thread);
}

#define TZ_TASK_CREATE_THREAD test_create
#define TZ_TASK_WAIT_THREAD test_wait
#define TZ_TASK_CLOSE_THREAD test_close
#define TZ_TASK_SYSCONF(name) 4
#include "../src/runtime/task.c"

static atomic_uint visits[257];
static void visit(void *context, uint64_t index) { (void)context; atomic_fetch_add(&visits[index], 1); }
static uint32_t result(void *context, uint64_t index) { visit(context, index); return index == 2 || index == 7; }

int main(int argc, char **argv) {
    SetErrorMode(SEM_FAILCRITICALERRORS | SEM_NOGPFAULTERRORBOX);
    _set_abort_behavior(0, _WRITE_ABORT_MSG | _CALL_REPORTFAULT);
    operation = argc > 1 ? argv[1] : "";
    tsuzuri_task_parallel(visit, NULL, 257);
    for (unsigned index = 0; index < 257; index++) assert(atomic_load(&visits[index]) == 1);
    assert(tsuzuri_task_parallel_results(result, NULL, 257) == 2);
    tz_task_shutdown();
    return 0;
}
