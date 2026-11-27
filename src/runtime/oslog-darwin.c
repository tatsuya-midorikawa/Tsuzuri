/* Report destination of Apple executables (M19). Linked into the executables Tsuzuri builds for an
   Apple target (build --emit exe, run), never into objects or shared libraries: a host that links an
   object keeps its own standard error. A process started by LaunchServices (Finder, the Dock, `open`)
   or by launchd has /dev/null as its standard error, so the report of a trap, of the tzroutine
   runtime or of a stack overflow would be lost there. When standard error is closed or /dev/null,
   and only then, the same text goes to unified logging (subsystem dev.tsuzuri.runtime), which
   Console.app and `log show` read. Every other destination keeps the bytes it got before. */
#if defined(__APPLE__)
#include <errno.h>
#include <stdatomic.h>
#include <stdint.h>
#include <string.h>
#include <sys/stat.h>
#if defined(TZ_REPORT_TEST_LOG)
/* Only tests/macos_report_host.c defines it: the entries go to that file, so that the decision can
   be checked where unified logging cannot be read. The compiler never defines it. */
#include <fcntl.h>
#include <unistd.h>
#else
/* What os_log_with_type of <os/log.h> expands to. The header is not included, because the toolchain
   of a distribution (zig's C library headers) does not have it; the functions are in libSystem since
   macOS 10.12. Clang encodes the arguments (__builtin_os_log_format), as it does for the header's
   macro, and the format string goes to the section that the decoder of the log reads.
   The image that sends is named by its Mach-O header. The macro passes &__dso_handle, which Apple's
   linker places at the header but the distribution's linker (zig) places in __DATA, and an entry
   sent with that address cannot be decoded. This file is linked into executables only, whose header
   is _mh_execute_header (<mach-o/ldsyms.h>) with both linkers. */
struct os_log_s;
struct mach_header_64;
extern const struct mach_header_64 _mh_execute_header;
struct os_log_s *os_log_create(const char *subsystem, const char *category);
_Bool os_log_type_enabled(struct os_log_s *log, uint8_t type);
void _os_log_impl(void *dso, struct os_log_s *log, uint8_t type, const char *format, uint8_t *buffer, uint32_t size);
enum { TZ_OS_LOG_TYPE_ERROR = 0x10 };
#define TZ_REPORT_FORMAT "%{public}.*s"
#endif

static _Atomic int32_t tz_report_state; /* 0 unknown, 1 standard error, 2 unified logging */

/* Asked before each report. The first call looks at fd 2 once and the answer stays: a process without
   a report never asks. fstat and stat are async-signal-safe, and two threads that ask at the same
   time store the same answer. */
__attribute__((visibility("hidden"), cold))
int32_t tsuzuri_report_to_log(void) {
    int32_t state = atomic_load_explicit(&tz_report_state, memory_order_relaxed);
    if (state == 0) {
        struct stat fd2, null;
        state = 1;
        if (fstat(2, &fd2) != 0) state = errno == EBADF ? 2 : 1;
        else if (S_ISCHR(fd2.st_mode) && stat("/dev/null", &null) == 0 && fd2.st_rdev == null.st_rdev) state = 2;
        atomic_store_explicit(&tz_report_state, state, memory_order_relaxed);
    }
    return state == 2;
}

static void tz_report_send(int32_t category, const char *text, int64_t length) {
    if (length > INT32_MAX) length = INT32_MAX;
#if defined(TZ_REPORT_TEST_LOG)
    int file = open(TZ_REPORT_TEST_LOG, O_WRONLY | O_CREAT | O_APPEND, 0600);
    if (file < 0) return;
    const char *name = category == 0 ? "trap\t" : "runtime\t";
    (void)!write(file, name, strlen(name));
    (void)!write(file, text, (size_t)length);
    (void)!write(file, "\n", 1);
    close(file);
#else
    /* os_log_create returns the same object for the same pair and never frees it; a process reports
       a few times in its life. The text has no secret (a reason and a source position), so it is
       public, and it is not NUL-terminated, so its length is passed. */
    struct os_log_s *log = os_log_create("dev.tsuzuri.runtime", category == 0 ? "trap" : "runtime");
    if (os_log_type_enabled(log, TZ_OS_LOG_TYPE_ERROR)) {
        __attribute__((section("__TEXT,__oslogstring,cstring_literals")))
        static const char format[] = TZ_REPORT_FORMAT;
        _Alignas(16) uint8_t buffer[__builtin_os_log_format_buffer_size(TZ_REPORT_FORMAT, (int)length, text)];
        _os_log_impl((void *)&_mh_execute_header, log, TZ_OS_LOG_TYPE_ERROR, format,
                     (uint8_t *)__builtin_os_log_format(buffer, TZ_REPORT_FORMAT, (int)length, text),
                     (uint32_t)sizeof buffer);
    }
#endif
}

/* A report is one entry: its text without the newline that ends it. One that arrives in pieces (the
   weak-symbol report writes a prefix, a name and a suffix) is held until the piece that ends with the
   newline. The holder is taken without waiting, so a signal handler that interrupts it, or a second
   thread, sends its own text at once. */
enum { TZ_REPORT_HELD = 512 };
static atomic_flag tz_report_holding = ATOMIC_FLAG_INIT;
static char tz_report_held[TZ_REPORT_HELD];
static size_t tz_report_held_length;

__attribute__((visibility("hidden"), cold))
void tsuzuri_report_log(int32_t category, const char *text, int64_t length) {
    if (length <= 0) return;
    int complete = text[length - 1] == '\n';
    if (complete) length--;
    if (atomic_flag_test_and_set_explicit(&tz_report_holding, memory_order_acquire)) {
        tz_report_send(category, text, length);
        return;
    }
    if (!complete || tz_report_held_length != 0) {
        size_t room = TZ_REPORT_HELD - tz_report_held_length;
        size_t count = (uint64_t)length < room ? (size_t)length : room;
        memcpy(tz_report_held + tz_report_held_length, text, count);
        tz_report_held_length += count;
        if (complete) {
            tz_report_send(category, tz_report_held, (int64_t)tz_report_held_length);
            tz_report_held_length = 0;
        }
    } else {
        tz_report_send(category, text, length);
    }
    atomic_flag_clear_explicit(&tz_report_holding, memory_order_release);
}
#endif
