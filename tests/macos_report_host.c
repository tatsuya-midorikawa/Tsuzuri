/* M19: the C side of tests/macos_report.mjs, which compiles this file in two ways.

   With -DTZ_REPORT_TEST_LOG="\"<file>\"" it includes src/runtime/oslog-darwin.c, whose entries then
   go to <file> instead of unified logging: the test checks where a report goes for each kind of
   standard error, and what an entry is, on a machine whose unified logging cannot be read.

   With -DTZ_REPORT_PROBE it writes one entry with the SDK's own os_log, by which the test learns
   whether `log show` can read the entries of its child processes at all. */
#if defined(TZ_REPORT_PROBE)
#include <os/log.h>

int main(void) {
    os_log_with_type(os_log_create("dev.tsuzuri.runtime", "probe"), OS_LOG_TYPE_ERROR, "%{public}s",
                     "tsuzuri macos_report probe");
    return 0;
}
#else
#include "../src/runtime/oslog-darwin.c"
#include <stdio.h>

static void piece(int32_t category, const char *text) {
    tsuzuri_report_log(category, text, (int64_t)strlen(text));
}

int main(void) {
    static char wide[601];
    int first = tsuzuri_report_to_log();
    /* The answer is remembered: pointing standard error somewhere else afterwards does not change it. */
    int null = open("/dev/null", O_WRONLY);
    int file = open(TZ_REPORT_TEST_LOG ".stderr", O_WRONLY | O_CREAT | O_TRUNC, 0600);
    if (null < 0 || file < 0 || dup2(first ? file : null, 2) < 0) return 2;
    printf("%d %d\n", first, tsuzuri_report_to_log());
    /* A whole report, a report in three pieces, a runtime report, a report longer than the holder,
       and a piece that no newline ends (never sent). */
    piece(0, "trap: integer division by zero at Main.tz:1:34\n");
    piece(0, "Tsuzuri runtime: the weak symbol '");
    piece(0, "tz_missing");
    piece(0, "' is not available on this system\n");
    piece(1, "Tsuzuri runtime: negative WaitGroup counter\n");
    memset(wide, 'x', sizeof wide - 1);
    piece(1, wide);
    piece(1, "\n");
    piece(0, "left");
    return 0;
}
#endif
