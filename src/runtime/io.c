#include <errno.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

#if defined(_WIN32)
#include <fcntl.h>
#include <io.h>
#include <stdlib.h>
#define TZ_IO_API
static void __cdecl tz_io_quiet(const wchar_t *expr, const wchar_t *func, const wchar_t *file, unsigned int line, uintptr_t reserved) {
    (void)expr;
    (void)func;
    (void)file;
    (void)line;
    (void)reserved;
}
_invalid_parameter_handler __cdecl _set_thread_local_invalid_parameter_handler(_invalid_parameter_handler);
#else
#include <fcntl.h>
#include <unistd.h>
#define TZ_IO_API __attribute__((weak, visibility("hidden")))
#endif

/* W08: whether the standard stream `fd` does not exist, asked only after a read or write on it failed. */
static int tz_io_absent(int fd) {
#if defined(_WIN32)
    errno = 0;
    intptr_t handle = _get_osfhandle(fd); /* under tz_io_quiet: a closed descriptor gives -1 and EBADF */
    return handle == -2 || (handle == -1 && errno == EBADF); /* -2 is _NO_CONSOLE_FILENO: no handle at startup */
#else
    int saved = errno;
    int absent = fcntl(fd, F_GETFD) == -1 && errno == EBADF;
    errno = saved;
    return absent;
#endif
}

extern void *tsuzuri_alloc(int64_t size);
extern void tsuzuri_free(void *pointer);

struct tz_io_buffer {
    unsigned char *data;
    int64_t length;
};

/* The blocking-call hooks (TZ08): os.c, io.c, net.c and gpu.c call them around a call that may wait
 * for the OS, and the scheduler sets them when it starts (tzr_init), so that a tzroutine in such a
 * call lets sysmon give its P away. The parts of the runtime are one translation unit, so the first
 * part that has this block defines them; a program without tzroutines leaves them null. */
#ifndef TSUZURI_BLOCKING_HOOKS_DEFINED
#define TSUZURI_BLOCKING_HOOKS_DEFINED
#include <stdatomic.h>
/* Windows has no weak data: a selectany definition is shared by the units that define it (the runner of
 * `tsuzuri test` compiles each part of the runtime alone). */
#if defined(_WIN32)
__attribute__((selectany)) void (*_Atomic tsuzuri_blocking_enter_hook)(void) = 0;
__attribute__((selectany)) void (*_Atomic tsuzuri_blocking_exit_hook)(void) = 0;
#else
__attribute__((weak, visibility("hidden"))) void (*_Atomic tsuzuri_blocking_enter_hook)(void);
__attribute__((weak, visibility("hidden"))) void (*_Atomic tsuzuri_blocking_exit_hook)(void);
#endif
#define TZ_BLOCKING_CALL(hook)                                                                   \
    do {                                                                                         \
        void (*tz_blocking_hook)(void) = atomic_load_explicit(&(hook), memory_order_acquire);   \
        if (tz_blocking_hook != 0) tz_blocking_hook();                                          \
    } while (0)
#define TZ_BLOCKING_ENTER() TZ_BLOCKING_CALL(tsuzuri_blocking_enter_hook)
#define TZ_BLOCKING_EXIT() TZ_BLOCKING_CALL(tsuzuri_blocking_exit_hook)
#endif

static int32_t tz_io_read_line_inner(struct tz_io_buffer *output) {
    output->data = NULL;
    output->length = 0;
#if defined(_WIN32)
    if (_setmode(0, _O_BINARY) == -1) return tz_io_absent(0) ? 3 : 2;
#endif
    int64_t capacity = 0;
    for (;;) {
        errno = 0;
        int value = fgetc(stdin);
        if (value == EOF) {
            if (ferror(stdin)) {
                if (errno == EINTR) {
                    clearerr(stdin);
                    continue;
                }
                tsuzuri_free(output->data);
                output->data = NULL;
                output->length = 0;
                return tz_io_absent(0) ? 3 : 2;
            }
            return output->length == 0 ? 1 : 0;
        }
        if (value == '\n') {
            if (output->length > 0 && output->data[output->length - 1] == '\r') {
                output->length--;
            }
            return 0;
        }
        if (output->length == capacity) {
            if (capacity > INT64_MAX / 2) {
                tsuzuri_free(output->data);
                output->data = NULL;
                output->length = 0;
                return 2;
            }
            int64_t next_capacity = capacity == 0 ? 256 : capacity * 2;
            unsigned char *next = tsuzuri_alloc(next_capacity);
            if (output->length > 0) memcpy(next, output->data, (size_t)output->length);
            tsuzuri_free(output->data);
            output->data = next;
            capacity = next_capacity;
        }
        output->data[output->length++] = (unsigned char)value;
    }
}

static int32_t tz_io_read_line_now(struct tz_io_buffer *output) {
#if defined(_WIN32)
    _invalid_parameter_handler previous = _set_thread_local_invalid_parameter_handler(tz_io_quiet);
    int32_t status = tz_io_read_line_inner(output);
    _set_thread_local_invalid_parameter_handler(previous);
    return status;
#else
    return tz_io_read_line_inner(output);
#endif
}

// TZ08: a tzroutine in this call lets its P go.
TZ_IO_API int32_t tsuzuri_io_read_line(struct tz_io_buffer *output) {
    TZ_BLOCKING_ENTER();
    int32_t status = tz_io_read_line_now(output);
    TZ_BLOCKING_EXIT();
    return status;
}

static int32_t tz_io_write_inner(int32_t destination, const unsigned char *data, int64_t length) {
    if ((destination != 1 && destination != 2) || length < 0) return 1;
#if defined(_WIN32)
    if (_setmode(destination, _O_BINARY) == -1) return tz_io_absent(destination) ? 2 : 1;
#else
    if (length == 0 && tz_io_absent(destination)) return 2;
#endif
    FILE *stream = destination == 1 ? stdout : stderr;
    int64_t offset = 0;
    while (offset < length) {
        size_t written = fwrite(data + offset, 1, (size_t)(length - offset), stream);
        offset += (int64_t)written;
        if (ferror(stream) || written == 0) return tz_io_absent(destination) ? 2 : 1;
    }
    return fflush(stream) == 0 ? 0 : (tz_io_absent(destination) ? 2 : 1);
}

static int32_t tz_io_write_now(int32_t destination, const unsigned char *data, int64_t length) {
#if defined(_WIN32)
    _invalid_parameter_handler previous = _set_thread_local_invalid_parameter_handler(tz_io_quiet);
    int32_t status = tz_io_write_inner(destination, data, length);
    _set_thread_local_invalid_parameter_handler(previous);
    return status;
#else
    return tz_io_write_inner(destination, data, length);
#endif
}

// TZ08: a tzroutine in this call lets its P go.
TZ_IO_API int32_t tsuzuri_io_write(int32_t destination, const unsigned char *data, int64_t length) {
    TZ_BLOCKING_ENTER();
    int32_t status = tz_io_write_now(destination, data, length);
    TZ_BLOCKING_EXIT();
    return status;
}
