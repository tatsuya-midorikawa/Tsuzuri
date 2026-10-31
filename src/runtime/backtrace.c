/* Crash report of a trap (B10 Phase 1): the frames that led to it, for native executables built with
   --trap-info (build, run). Linked into executables only, never into objects, so a host that links an
   object keeps its own process. The generated @tz.trap.report calls tsuzuri_trap_backtrace(site) after
   it wrote the site's message. Nothing is written unless TSUZURI_TRAP_REPORT names a file; `tsuzuri run`
   sets it. The code walks the chain of frame pointers: every function that Tsuzuri generates keeps one in
   such a build ("frame-pointer"="all"), and a frame record is the saved frame pointer followed by the
   return address on arm64 and x86-64. Every record is read through a pipe, which cannot fault. */
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
#include <string.h>
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

/* Reads the frame record (the saved frame pointer and the return address) at `address` without a
   fault. The words pass through a pipe: write() fails with EFAULT for memory that the process cannot
   read, unmapped or PROT_NONE alike, where a load would end the process before the report is written
   (mincore() says only that a page is mapped). A record that runs into an unreadable page is written
   in part; what was written is drained, and the record is not read. 1 when both words were read. */
static int tz_backtrace_read(const int pipe_fds[2], uintptr_t address, uintptr_t record[2]) {
    const size_t size = 2 * sizeof(uintptr_t);
    if (address == 0 || (address & (sizeof(uintptr_t) - 1)) != 0) return 0;
    ssize_t written = write(pipe_fds[1], (const void *)address, size);
    if (written <= 0) return 0;
    unsigned char bytes[2 * sizeof(uintptr_t)];
    ssize_t drained = read(pipe_fds[0], bytes, (size_t)written);
    if (written != (ssize_t)size || drained != written) return 0;
    memcpy(record, bytes, size);
    return 1;
}

/* Follows the chain of frame records from `record`: the first `skip` return addresses are left out,
   the next `capacity` are stored innermost first, and `*truncated` says that more were left. The chain
   ends at a return address of 0, a record that cannot be read or is not aligned, or a caller's record
   that is not above its callee's (a stack grows down). Returns the number stored. */
static unsigned tz_backtrace_walk(uintptr_t record, unsigned skip, uintptr_t *frames, unsigned capacity,
                                  int *truncated) {
    int pipe_fds[2];
    unsigned count = 0;
    *truncated = 0;
    if (pipe(pipe_fds) != 0) return 0;
    uintptr_t words[2];
    while (tz_backtrace_read(pipe_fds, record, words)) {
        uintptr_t next = words[0];
        uintptr_t address = words[1];
        if (address == 0) break;
        if (skip) --skip;
        else if (count < capacity) frames[count++] = address;
        else {
            *truncated = 1;
            break;
        }
        if (next <= record) break;
        record = next;
    }
    (void)close(pipe_fds[0]);
    (void)close(pipe_fds[1]);
    return count;
}

/* Appends one line `tz-trap/1 <site> <slide> <truncated> <address>...` (hexadecimal with 0x, the
   innermost frame first) to the file of TSUZURI_TRAP_REPORT. */
__attribute__((noinline, visibility("hidden")))
void tsuzuri_trap_backtrace(uint32_t site) {
    const char *path = getenv("TSUZURI_TRAP_REPORT");
    if (!path || !*path) return;
    uintptr_t frames[TZ_BACKTRACE_FRAMES];
    int truncated = 0;
    /* The first return address is into @tz.trap.report itself. */
    unsigned count = tz_backtrace_walk((uintptr_t)__builtin_frame_address(0), 1, frames,
                                       TZ_BACKTRACE_FRAMES, &truncated);
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
