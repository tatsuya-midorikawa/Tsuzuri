/* W11: small Win32 reference helper for tests/os.mjs on Windows.
 *   hold <utf8-path>  -- opens <path> with dwShareMode = 0, prints "ready\n", and waits for stdin EOF.
 */
#undef NDEBUG
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#ifndef _CRT_SECURE_NO_WARNINGS
#define _CRT_SECURE_NO_WARNINGS
#endif
#include <windows.h>
#include <stdio.h>
#include <string.h>

int main(int argc, char **argv) {
    if (argc == 3 && strcmp(argv[1], "hold") == 0) {
        wchar_t wide[4096];
        int n = MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, argv[2], -1, wide, 4096);
        if (n <= 0) return 2;
        HANDLE h = CreateFileW(wide, GENERIC_READ | GENERIC_WRITE, 0, NULL, OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, NULL);
        if (h == INVALID_HANDLE_VALUE) {
            fprintf(stderr, "CreateFileW failed: %lu\n", GetLastError());
            return 3;
        }
        printf("ready\n");
        fflush(stdout);
        (void)getchar();
        CloseHandle(h);
        return 0;
    }
    fprintf(stderr, "usage: os_windows_reference hold <path>\n");
    return 1;
}
