/* W33: prints the real UCRT `wmain` `argv[1..]` as `[arg1][arg2]...\n` in UTF-8 (binary stdout)
 * and exits with `argc - 1`, matching the Tsuzuri differential harness in `tests/windows.mjs`. */
#undef NDEBUG
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#ifndef _CRT_SECURE_NO_WARNINGS
#define _CRT_SECURE_NO_WARNINGS
#endif
#include <windows.h>
#include <fcntl.h>
#include <io.h>
#include <stdio.h>
#include <stdlib.h>

int wmain(int argc, wchar_t **argv) {
    if (_setmode(_fileno(stdout), _O_BINARY) == -1) return 250;
    fputc('[', stdout);
    for (int index = 1; index < argc; index++) {
        if (index > 1) fputs("][", stdout);
        int units = (int)wcslen(argv[index]);
        if (units > 0) {
            int bytes = WideCharToMultiByte(CP_UTF8, 0, argv[index], units, NULL, 0, NULL, NULL);
            if (bytes <= 0) return 251;
            char *buffer = (char *)malloc((size_t)bytes);
            if (buffer == NULL) return 252;
            if (WideCharToMultiByte(CP_UTF8, 0, argv[index], units, buffer, bytes, NULL, NULL) != bytes) {
                free(buffer);
                return 253;
            }
            if (fwrite(buffer, 1, (size_t)bytes, stdout) != (size_t)bytes) {
                free(buffer);
                return 254;
            }
            free(buffer);
        }
    }
    fputs("]\n", stdout);
    if (fflush(stdout) != 0) return 255;
    return argc > 1 ? argc - 1 : 0;
}
