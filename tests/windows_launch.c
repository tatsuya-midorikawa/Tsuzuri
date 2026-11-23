/* W04 / W06: launches a Windows child process with or without standard handles, optionally
 * capturing OutputDebugStringW/A messages as UTF-8 lines via WaitForDebugEventEx. */
#undef NDEBUG
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#ifndef _CRT_SECURE_NO_WARNINGS
#define _CRT_SECURE_NO_WARNINGS
#endif
#include <windows.h>
#include <psapi.h>
#include <io.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static void format_exception_site(HANDLE process, const void *address, unsigned long long *rva_out, char *module_out, size_t module_cap) {
    *rva_out = (unsigned long long)(uintptr_t)address;
    strncpy(module_out, "<unknown>", module_cap);
    module_out[module_cap - 1] = 0;
    HMODULE modules[256];
    DWORD needed = 0;
    if (!K32EnumProcessModules(process, modules, sizeof(modules), &needed)) return;
    DWORD count = needed / sizeof(HMODULE);
    if (count > 256) count = 256;
    uintptr_t target = (uintptr_t)address;
    for (DWORD i = 0; i < count; i++) {
        MODULEINFO info;
        if (!K32GetModuleInformation(process, modules[i], &info, sizeof(info))) continue;
        uintptr_t base = (uintptr_t)info.lpBaseOfDll;
        if (target >= base && target < base + (uintptr_t)info.SizeOfImage) {
            *rva_out = (unsigned long long)(target - base);
            if (!K32GetModuleBaseNameA(process, modules[i], module_out, (DWORD)module_cap)) {
                strncpy(module_out, "<module>", module_cap);
                module_out[module_cap - 1] = 0;
            }
            return;
        }
    }
}

static const wchar_t *skip_spaces(const wchar_t *p) {
    while (*p == L' ' || *p == L'\t') p++;
    return p;
}

static const wchar_t *next_token(const wchar_t *p, wchar_t *out, size_t cap) {
    p = skip_spaces(p);
    if (*p == 0) {
        out[0] = 0;
        return p;
    }
    size_t len = 0;
    int in_quotes = 0;
    while (*p != 0 && (in_quotes || (*p != L' ' && *p != L'\t'))) {
        if (*p == L'"') {
            in_quotes = !in_quotes;
            p++;
            continue;
        }
        if (len + 1 < cap) out[len++] = *p;
        p++;
    }
    out[len] = 0;
    return p;
}

static void append_debug_line(const wchar_t *path, const char *utf8, int length) {
    while (length > 0 && (utf8[length - 1] == '\n' || utf8[length - 1] == '\r')) {
        length--;
    }
    HANDLE file = CreateFileW(
        path,
        FILE_APPEND_DATA,
        FILE_SHARE_READ | FILE_SHARE_WRITE,
        NULL,
        OPEN_ALWAYS,
        FILE_ATTRIBUTE_NORMAL,
        NULL
    );
    if (file == INVALID_HANDLE_VALUE) return;
    DWORD written = 0;
    if (length > 0) (void)WriteFile(file, utf8, (DWORD)length, &written, NULL);
    (void)WriteFile(file, "\n", 1, &written, NULL);
    CloseHandle(file);
}

static void capture_debug_string(HANDLE process, const OUTPUT_DEBUG_STRING_INFO *info, const wchar_t *out_path) {
    if (info->lpDebugStringData == NULL) return;
    if (info->fUnicode) {
        WCHAR wide[65536];
        size_t index = 0;
        while (index + 1 < 65536) {
            WCHAR ch = 0;
            SIZE_T read_bytes = 0;
            if (!ReadProcessMemory(process, (const char *)info->lpDebugStringData + index * sizeof(WCHAR), &ch, sizeof(WCHAR), &read_bytes) || read_bytes != sizeof(WCHAR) || ch == 0) {
                break;
            }
            wide[index++] = ch;
        }
        wide[index] = 0;
        char utf8[65536 * 3];
        int bytes = WideCharToMultiByte(CP_UTF8, 0, wide, (int)index, utf8, (int)sizeof(utf8), NULL, NULL);
        if (bytes >= 0) append_debug_line(out_path, utf8, bytes);
    } else {
        char ansi[65536];
        size_t index = 0;
        while (index + 1 < 65536) {
            char ch = 0;
            SIZE_T read_bytes = 0;
            if (!ReadProcessMemory(process, (const char *)info->lpDebugStringData + index, &ch, 1, &read_bytes) || read_bytes != 1 || ch == 0) {
                break;
            }
            ansi[index++] = ch;
        }
        ansi[index] = 0;
        WCHAR wide[65536];
        int wlen = MultiByteToWideChar(CP_ACP, 0, ansi, (int)index, wide, 65535);
        if (wlen >= 0) {
            char utf8[65536 * 3];
            int bytes = WideCharToMultiByte(CP_UTF8, 0, wide, wlen, utf8, (int)sizeof(utf8), NULL, NULL);
            if (bytes >= 0) append_debug_line(out_path, utf8, bytes);
        }
    }
}

static int run_probe(const wchar_t *path) {
    FILE *file = _wfopen(path, L"wb");
    if (!file) return 2;
    const DWORD std_ids[3] = { STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE };
    for (int fd = 0; fd < 3; fd++) {
        intptr_t crt = _get_osfhandle(fd);
        HANDLE os = GetStdHandle(std_ids[fd]);
        fprintf(file, "fd%d=%lld handle%d=%p\n", fd, (long long)crt, fd, (void *)os);
    }
    fclose(file);
    return 0;
}

int main(void) {
    SetErrorMode(SEM_FAILCRITICALERRORS | SEM_NOGPFAULTERRORBOX);
    const wchar_t *cursor = GetCommandLineW();
    wchar_t token[4096];
    cursor = next_token(cursor, token, 4096); /* program name */

    int no_handles = 0;
    int first_exception = 0;
    wchar_t debug_strings_path[4096] = { 0 };
    DWORD timeout_ms = 60000;

    while (1) {
        const wchar_t *before = skip_spaces(cursor);
        if (*before == 0) {
            fprintf(stderr, "windows_launch: missing -- <command>\n");
            return 2;
        }
        cursor = next_token(before, token, 4096);
        if (wcscmp(token, L"--") == 0) {
            cursor = skip_spaces(cursor);
            break;
        } else if (wcscmp(token, L"--no-handles") == 0) {
            no_handles = 1;
        } else if (wcscmp(token, L"--first-exception") == 0) {
            first_exception = 1;
        } else if (wcscmp(token, L"--debug-strings") == 0) {
            cursor = next_token(cursor, debug_strings_path, 4096);
        } else if (wcscmp(token, L"--timeout-ms") == 0) {
            wchar_t num[64];
            cursor = next_token(cursor, num, 64);
            timeout_ms = (DWORD)wcstoul(num, NULL, 10);
        } else if (wcscmp(token, L"--probe") == 0) {
            wchar_t probe_path[4096];
            (void)next_token(cursor, probe_path, 4096);
            return run_probe(probe_path);
        } else if (wcscmp(token, L"--probe-debug") == 0) {
            OutputDebugStringW(L"probe 1");
            OutputDebugStringW(L"probe 2");
            return 0;
        } else {
            fprintf(stderr, "windows_launch: unknown option\n");
            return 2;
        }
    }

    if (*cursor == 0) {
        fprintf(stderr, "windows_launch: empty command\n");
        return 2;
    }

    size_t cmd_len = wcslen(cursor);
    wchar_t *cmd_line = (wchar_t *)malloc((cmd_len + 1) * sizeof(wchar_t));
    if (!cmd_line) return 2;
    memcpy(cmd_line, cursor, (cmd_len + 1) * sizeof(wchar_t));

    if (debug_strings_path[0] != 0) {
        HANDLE f = CreateFileW(debug_strings_path, GENERIC_WRITE, FILE_SHARE_READ | FILE_SHARE_WRITE, NULL, CREATE_ALWAYS, FILE_ATTRIBUTE_NORMAL, NULL);
        if (f != INVALID_HANDLE_VALUE) CloseHandle(f);
    }

    int debug = first_exception || (debug_strings_path[0] != 0);
    STARTUPINFOW startup;
    ZeroMemory(&startup, sizeof startup);
    startup.cb = sizeof startup;
    DWORD flags = debug ? DEBUG_ONLY_THIS_PROCESS : 0;
    BOOL inherit = TRUE;
    if (no_handles) {
        startup.dwFlags = STARTF_USESTDHANDLES;
        startup.hStdInput = NULL;
        startup.hStdOutput = NULL;
        startup.hStdError = NULL;
        inherit = FALSE;
        flags |= DETACHED_PROCESS;
    }

    PROCESS_INFORMATION process;
    ZeroMemory(&process, sizeof process);
    if (!CreateProcessW(NULL, cmd_line, NULL, NULL, inherit, flags, NULL, NULL, &startup, &process)) {
        fprintf(stderr, "CreateProcessW failed: %lu\n", GetLastError());
        free(cmd_line);
        return 2;
    }
    free(cmd_line);

    ULONGLONG deadline = GetTickCount64() + timeout_ms;
    int seen_initial_bp = 0;
    int reported_exception = 0;
    DWORD exit_code = 0;

    if (debug) {
        int exited = 0;
        while (!exited) {
            ULONGLONG now = GetTickCount64();
            if (now >= deadline) {
                TerminateProcess(process.hProcess, 1);
                CloseHandle(process.hThread);
                CloseHandle(process.hProcess);
                printf("timeout\n");
                return 1;
            }
            DWORD slice = (DWORD)(deadline - now > 250 ? 250 : deadline - now);
            DEBUG_EVENT event;
            if (!WaitForDebugEventEx(&event, slice)) {
                if (GetLastError() == ERROR_SEM_TIMEOUT) continue;
                break;
            }
            DWORD status = DBG_CONTINUE;
            switch (event.dwDebugEventCode) {
            case OUTPUT_DEBUG_STRING_EVENT:
                if (debug_strings_path[0] != 0) {
                    capture_debug_string(process.hProcess, &event.u.DebugString, debug_strings_path);
                }
                break;
            case EXCEPTION_DEBUG_EVENT: {
                DWORD code = event.u.Exception.ExceptionRecord.ExceptionCode;
                if (code == EXCEPTION_BREAKPOINT && !seen_initial_bp) {
                    seen_initial_bp = 1;
                    status = DBG_CONTINUE;
                } else {
                    if (first_exception && !reported_exception && event.u.Exception.dwFirstChance && code != 0x40010006u && code != 0x4001000Au) {
                        reported_exception = 1;
                        unsigned long long rva = 0;
                        char mod_name[260];
                        format_exception_site(process.hProcess, event.u.Exception.ExceptionRecord.ExceptionAddress, &rva, mod_name, sizeof(mod_name));
                        printf("exception=0x%08lX rva=0x%llx module=%s\n", code, rva, mod_name);
                    }
                    status = DBG_EXCEPTION_NOT_HANDLED;
                }
                break;
            }
            case EXIT_PROCESS_DEBUG_EVENT:
                exit_code = event.u.ExitProcess.dwExitCode;
                exited = 1;
                break;
            default:
                break;
            }
            ContinueDebugEvent(event.dwProcessId, event.dwThreadId, status);
        }
    } else {
        DWORD waited = WaitForSingleObject(process.hProcess, timeout_ms);
        if (waited == WAIT_TIMEOUT) {
            TerminateProcess(process.hProcess, 1);
            CloseHandle(process.hThread);
            CloseHandle(process.hProcess);
            printf("timeout\n");
            return 1;
        }
        GetExitCodeProcess(process.hProcess, &exit_code);
    }

    CloseHandle(process.hThread);
    CloseHandle(process.hProcess);
    printf("exit=0x%08lX\n", exit_code);
    return 0;
}
