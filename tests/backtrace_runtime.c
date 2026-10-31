/* Unit tests of the crash report's walk (B10, src/runtime/backtrace.c): records are read through a pipe,
   so a chain that runs into memory the process cannot read (unmapped or PROT_NONE) ends there instead of
   faulting. The chains are built by hand in one mapping whose second page is made PROT_NONE.
   Usage: backtrace_runtime <report file> */
#include "../src/runtime/backtrace.c"
#undef NDEBUG
#include <assert.h>
#include <sys/mman.h>

static unsigned checks;
#define CHECK(condition) do { assert(condition); ++checks; } while (0)

/* Writes the record (next, address) at `record`. */
static void link_record(uintptr_t *record, uintptr_t next, uintptr_t address) {
    record[0] = next;
    record[1] = address;
}

__attribute__((noinline)) static void report_here(void) {
    tsuzuri_trap_backtrace(7);
}

int main(int argc, char **argv) {
    long page = sysconf(_SC_PAGESIZE);
    CHECK(page > 0);
    unsigned char *mapping = mmap(NULL, 2 * (size_t)page, PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    CHECK(mapping != MAP_FAILED);
    uintptr_t base = (uintptr_t)mapping;
    uintptr_t guard = base + (uintptr_t)page;
    CHECK(mprotect((void *)guard, (size_t)page, PROT_NONE) == 0);
    int pipe_fds[2];
    CHECK(pipe(pipe_fds) == 0);
    uintptr_t words[2];

    /* A readable record comes back whole; an unaligned, null, PROT_NONE or straddling one is not read. */
    uintptr_t *first = (uintptr_t *)(base + 64);
    link_record(first, 0x1234, 0x5678);
    CHECK(tz_backtrace_read(pipe_fds, (uintptr_t)first, words) && words[0] == 0x1234 && words[1] == 0x5678);
    CHECK(!tz_backtrace_read(pipe_fds, (uintptr_t)first + 1, words));
    CHECK(!tz_backtrace_read(pipe_fds, 0, words));
    CHECK(!tz_backtrace_read(pipe_fds, guard, words));
    CHECK(!tz_backtrace_read(pipe_fds, guard - sizeof(uintptr_t), words));
    /* The pipe holds nothing afterwards: the next read is the next record, not left-over bytes. */
    link_record(first, 0xabc, 0xdef);
    CHECK(tz_backtrace_read(pipe_fds, (uintptr_t)first, words) && words[0] == 0xabc && words[1] == 0xdef);
    (void)close(pipe_fds[0]);
    (void)close(pipe_fds[1]);

    uintptr_t frames[TZ_BACKTRACE_FRAMES];
    int truncated = 0;
    /* A chain of three records whose last link points into the PROT_NONE page: three addresses, no fault. */
    uintptr_t *a = (uintptr_t *)(base + 128), *b = (uintptr_t *)(base + 512), *c = (uintptr_t *)(base + 1024);
    link_record(a, (uintptr_t)b, 0x1001);
    link_record(b, (uintptr_t)c, 0x1002);
    link_record(c, guard + 16, 0x1003);
    unsigned count = tz_backtrace_walk((uintptr_t)a, 0, frames, TZ_BACKTRACE_FRAMES, &truncated);
    CHECK(count == 3 && !truncated);
    CHECK(frames[0] == 0x1001 && frames[1] == 0x1002 && frames[2] == 0x1003);
    /* The first `skip` addresses are left out. */
    CHECK(tz_backtrace_walk((uintptr_t)a, 1, frames, TZ_BACKTRACE_FRAMES, &truncated) == 2 && frames[0] == 0x1002);
    /* A last record that straddles into the PROT_NONE page is not read. */
    link_record(c, guard - sizeof(uintptr_t), 0x1003);
    CHECK(tz_backtrace_walk((uintptr_t)a, 0, frames, TZ_BACKTRACE_FRAMES, &truncated) == 3);
    /* A link that goes down, a return address of 0, and an unaligned link end the chain. */
    link_record(c, (uintptr_t)a, 0x1003);
    CHECK(tz_backtrace_walk((uintptr_t)a, 0, frames, TZ_BACKTRACE_FRAMES, &truncated) == 3);
    link_record(b, (uintptr_t)c, 0);
    CHECK(tz_backtrace_walk((uintptr_t)a, 0, frames, TZ_BACKTRACE_FRAMES, &truncated) == 1);
    link_record(b, (uintptr_t)c + 1, 0x1002);
    CHECK(tz_backtrace_walk((uintptr_t)a, 0, frames, TZ_BACKTRACE_FRAMES, &truncated) == 2);
    /* A chain longer than the capacity is cut and says so. */
    for (unsigned index = 0; index < 40; ++index) {
        uintptr_t *record = (uintptr_t *)(base + 16 * (uintptr_t)index);
        link_record(record, base + 16 * (uintptr_t)(index + 1), 0x2000 + index);
    }
    count = tz_backtrace_walk(base, 0, frames, TZ_BACKTRACE_FRAMES, &truncated);
    CHECK(count == TZ_BACKTRACE_FRAMES && truncated && frames[31] == 0x2000 + 31);

    /* The report itself: one line of this program's real chain. */
    if (argc > 1) {
        CHECK(setenv("TSUZURI_TRAP_REPORT", argv[1], 1) == 0);
        report_here();
    }
    CHECK(munmap(mapping, 2 * (size_t)page) == 0);
    printf("backtrace runtime: %u checks passed\n", checks);
    return 0;
}
