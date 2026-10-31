/* Crash report of a trap (B10 Phase 1): the frames that led to it, for native executables built with
   --trap-info (build, run). Linked into executables only, never into objects, so a host that links an
   object keeps its own process. The generated @tz.trap.report calls tsuzuri_trap_backtrace(site) after
   it wrote the site's message. Nothing is written unless TSUZURI_TRAP_REPORT names a file; `tsuzuri run`
   sets it. The code walks the chain of frame pointers: every function that Tsuzuri generates keeps one in
   such a build ("frame-pointer"="all"), and a frame record is the saved frame pointer followed by the
   return address on arm64 and x86-64. Every record is checked to be mapped before it is read. */
#if defined(__APPLE__) && !defined(_DARWIN_C_SOURCE)
#define _DARWIN_C_SOURCE
#endif
#if defined(__linux__) && !defined(_GNU_SOURCE)
#define _GNU_SOURCE
#endif
#include <fcntl.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/mman.h>
#include <unistd.h>
#if defined(__APPLE__)
#include <mach-o/dyld.h>
#else
#include <link.h>
#endif

enum { TZ_BACKTRACE_FRAMES = 32 };

#if !defined(__APPLE__)
static int tz_backtrace_first_image(struct dl_phdr_info *info, size_t size, void *data) {
    (void)size;
    *(uintptr_t *)data = (uintptr_t)info->dlpi_addr;
    return 1;
}
#endif

/* The distance between where the executable is loaded and where it was linked. */
static uintptr_t tz_backtrace_slide(void) {
#if defined(__APPLE__)
    return (uintptr_t)_dyld_get_image_vmaddr_slide(0);
#else
    uintptr_t slide = 0;
    dl_iterate_phdr(tz_backtrace_first_image, &slide);
    return slide;
#endif
}

/* Whether the `length` bytes at `address` are mapped, so that reading them cannot fault. */
static int tz_backtrace_readable(uintptr_t address, size_t length) {
    long page = sysconf(_SC_PAGESIZE);
    if (page <= 0) return 0;
    uintptr_t mask = (uintptr_t)page - 1;
    uintptr_t start = address & ~mask;
    uintptr_t end = (address + length + mask) & ~mask;
    char vector[2];
    if (end <= start || (end - start) / (uintptr_t)page > sizeof vector) return 0;
    return mincore((void *)start, (size_t)(end - start), (void *)vector) == 0;
}

/* Appends one line `tz-trap/1 <site> <slide> <truncated> <address>...` (hexadecimal with 0x, the
   innermost frame first) to the file of TSUZURI_TRAP_REPORT. */
__attribute__((noinline, visibility("hidden")))
void tsuzuri_trap_backtrace(uint32_t site) {
    const char *path = getenv("TSUZURI_TRAP_REPORT");
    if (!path || !*path) return;
    uintptr_t frames[TZ_BACKTRACE_FRAMES];
    unsigned count = 0;
    int truncated = 0;
    int skipped = 0;
    uintptr_t *record = (uintptr_t *)__builtin_frame_address(0);
    while (record && ((uintptr_t)record & (sizeof(uintptr_t) - 1)) == 0
           && tz_backtrace_readable((uintptr_t)record, 2 * sizeof(uintptr_t))) {
        uintptr_t next = record[0];
        uintptr_t address = record[1];
        if (address == 0) break;
        /* The first return address is into @tz.trap.report itself. */
        if (!skipped) skipped = 1;
        else if (count < TZ_BACKTRACE_FRAMES) frames[count++] = address;
        else {
            truncated = 1;
            break;
        }
        /* A stack grows down, so the record of a caller lies above its callee's. */
        if (next <= (uintptr_t)record) break;
        record = (uintptr_t *)next;
    }
    char line[1024];
    int length = snprintf(line, sizeof line, "tz-trap/1 %u 0x%lx %d", (unsigned)site,
                          (unsigned long)tz_backtrace_slide(), truncated);
    for (unsigned index = 0; index < count && length > 0 && length < (int)sizeof line - 24; ++index) {
        length += snprintf(line + length, sizeof line - (size_t)length, " 0x%lx", (unsigned long)frames[index]);
    }
    if (length <= 0 || length >= (int)sizeof line - 1) return;
    line[length++] = '\n';
    int descriptor = open(path, O_WRONLY | O_CREAT | O_APPEND, 0600);
    if (descriptor < 0) return;
    (void)!write(descriptor, line, (size_t)length);
    (void)close(descriptor);
}
