// Windows half of src/runtime/os.c (W11 Phase 1 & Phase 2): Win32 implementations of the ten
// tsuzuri_os_* entry points for File, Dir, Env, Time, Random, and Process. Every helper is static
// and prefixed with tz_os_ so os.c can be concatenated ahead of net.c, io.c, arguments.c, and
// tzroutine.c in a single translation unit.
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#ifndef _CRT_SECURE_NO_WARNINGS
#define _CRT_SECURE_NO_WARNINGS
#endif
#include <windows.h>
#include <bcrypt.h>
#include <limits.h>
#if defined(_MSC_VER)
#pragma comment(lib, "bcrypt.lib")
#endif

#include "arguments-split.h"

#ifndef FILE_FLAG_OPEN_REPARSE_POINT
#define FILE_FLAG_OPEN_REPARSE_POINT 0x00200000
#endif
#ifndef PIPE_REJECT_REMOTE_CLIENTS
#define PIPE_REJECT_REMOTE_CLIENTS 0x00000008
#endif
#ifndef PROC_THREAD_ATTRIBUTE_HANDLE_LIST
#define PROC_THREAD_ATTRIBUTE_HANDLE_LIST 0x00020002
#endif
#ifndef EXTENDED_STARTUPINFO_PRESENT
#define EXTENDED_STARTUPINFO_PRESENT 0x00080000
#endif
#ifndef BCRYPT_USE_SYSTEM_PREFERRED_RNG
#define BCRYPT_USE_SYSTEM_PREFERRED_RNG 0x00000002
#endif
#ifndef BCRYPT_SUCCESS
#define BCRYPT_SUCCESS(Status) (((NTSTATUS)(Status)) >= 0)
#endif

static int64_t tz_os_win_error(DWORD error) {
    int kind;
    switch (error) {
    case ERROR_FILE_NOT_FOUND:
    case ERROR_PATH_NOT_FOUND:
        kind = TZ_OS_NOT_FOUND;
        break;
    case ERROR_ACCESS_DENIED:
    case ERROR_SHARING_VIOLATION:
        kind = TZ_OS_PERMISSION_DENIED;
        break;
    case ERROR_FILE_EXISTS:
    case ERROR_ALREADY_EXISTS:
        kind = TZ_OS_ALREADY_EXISTS;
        break;
    case ERROR_INVALID_PARAMETER:
    case ERROR_INVALID_NAME:
    case ERROR_FILENAME_EXCED_RANGE:
    case ERROR_DIRECTORY:
        kind = TZ_OS_INVALID_INPUT;
        break;
    case ERROR_NO_UNICODE_TRANSLATION:
        kind = TZ_OS_INVALID_ENCODING;
        break;
    default:
        kind = TZ_OS_OTHER;
        break;
    }
    return tz_os_status(kind, (int)error);
}

static int64_t tz_os_bad_handle(void) {
    return tz_os_status(TZ_OS_INVALID_INPUT, ERROR_INVALID_HANDLE);
}

static int tz_os_has_device_prefix(const wchar_t *path) {
    return path[0] == L'\\' && path[1] == L'\\' && (path[2] == L'?' || path[2] == L'.') && path[3] == L'\\';
}

// Converts a NUL-terminated UTF-8 string to a newly allocated UTF-16 string. When `suffix` is
// non-NULL (used for Dir.list's "\*"), it is appended after optional long-path expansion (D5:
// paths reaching 248 UTF-16 units without \\?\ or \\.\ are expanded via GetFullPathNameW and
// prefixed with \\?\ or \\?\UNC\).
static wchar_t *tz_os_wide_ex(const char *utf8, const wchar_t *suffix, int allow_long_prefix, int64_t *status) {
    size_t utf8_len = strlen(utf8);
    size_t suffix_len = suffix != NULL ? wcslen(suffix) : 0;
    if (utf8_len > (size_t)INT_MAX) {
        *status = tz_os_win_error(ERROR_FILENAME_EXCED_RANGE);
        return NULL;
    }
    int wlen = 0;
    wchar_t *base = NULL;
    if (utf8_len == 0) {
        base = (wchar_t *)malloc(sizeof(wchar_t));
        if (base == NULL) {
            *status = tz_os_status(TZ_OS_OTHER, ERROR_NOT_ENOUGH_MEMORY);
            return NULL;
        }
        base[0] = 0;
    } else {
        wlen = MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, utf8, (int)utf8_len, NULL, 0);
        if (wlen <= 0) {
            *status = tz_os_win_error(GetLastError());
            return NULL;
        }
        base = (wchar_t *)malloc(((size_t)wlen + 1) * sizeof(wchar_t));
        if (base == NULL) {
            *status = tz_os_status(TZ_OS_OTHER, ERROR_NOT_ENOUGH_MEMORY);
            return NULL;
        }
        if (MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, utf8, (int)utf8_len, base, wlen) <= 0) {
            DWORD err = GetLastError();
            free(base);
            *status = tz_os_win_error(err);
            return NULL;
        }
        base[wlen] = 0;
    }

    const wchar_t *effective_suffix = suffix;
    size_t effective_suffix_len = suffix_len;
    if (suffix != NULL && suffix[0] == L'\\' && wlen > 0 && (base[wlen - 1] == L'\\' || base[wlen - 1] == L'/')) {
        effective_suffix = suffix + 1;
        effective_suffix_len = suffix_len - 1;
    }

    if (allow_long_prefix && !tz_os_has_device_prefix(base) && (size_t)wlen + effective_suffix_len >= 248) {
        DWORD need = GetFullPathNameW(base, 0, NULL, NULL);
        if (need == 0) {
            DWORD err = GetLastError();
            free(base);
            *status = tz_os_win_error(err);
            return NULL;
        }
        wchar_t *full = (wchar_t *)malloc((size_t)need * sizeof(wchar_t));
        if (full == NULL) {
            free(base);
            *status = tz_os_status(TZ_OS_OTHER, ERROR_NOT_ENOUGH_MEMORY);
            return NULL;
        }
        DWORD got = GetFullPathNameW(base, need, full, NULL);
        free(base);
        if (got == 0 || got >= need) {
            DWORD err = got == 0 ? GetLastError() : ERROR_INSUFFICIENT_BUFFER;
            free(full);
            *status = tz_os_win_error(err);
            return NULL;
        }
        const wchar_t *suf = suffix;
        size_t suf_len = suffix_len;
        if (suffix != NULL && suffix[0] == L'\\' && got > 0 && (full[got - 1] == L'\\' || full[got - 1] == L'/')) {
            suf = suffix + 1;
            suf_len = suffix_len - 1;
        }
        int is_unc = full[0] == L'\\' && full[1] == L'\\' && !tz_os_has_device_prefix(full);
        const wchar_t *prefix = tz_os_has_device_prefix(full) ? L"" : (is_unc ? L"\\\\?\\UNC\\" : L"\\\\?\\");
        size_t prefix_len = wcslen(prefix);
        const wchar_t *body = is_unc ? full + 2 : full;
        size_t body_len = is_unc ? (size_t)got - 2 : (size_t)got;
        wchar_t *result = (wchar_t *)malloc((prefix_len + body_len + suf_len + 1) * sizeof(wchar_t));
        if (result == NULL) {
            free(full);
            *status = tz_os_status(TZ_OS_OTHER, ERROR_NOT_ENOUGH_MEMORY);
            return NULL;
        }
        if (prefix_len > 0) memcpy(result, prefix, prefix_len * sizeof(wchar_t));
        if (body_len > 0) memcpy(result + prefix_len, body, body_len * sizeof(wchar_t));
        if (suf_len > 0) memcpy(result + prefix_len + body_len, suf, suf_len * sizeof(wchar_t));
        result[prefix_len + body_len + suf_len] = 0;
        free(full);
        return result;
    }

    if (effective_suffix_len == 0) return base;
    wchar_t *result = (wchar_t *)malloc(((size_t)wlen + effective_suffix_len + 1) * sizeof(wchar_t));
    if (result == NULL) {
        free(base);
        *status = tz_os_status(TZ_OS_OTHER, ERROR_NOT_ENOUGH_MEMORY);
        return NULL;
    }
    if (wlen > 0) memcpy(result, base, (size_t)wlen * sizeof(wchar_t));
    memcpy(result + wlen, effective_suffix, effective_suffix_len * sizeof(wchar_t));
    result[(size_t)wlen + effective_suffix_len] = 0;
    free(base);
    return result;
}

static wchar_t *tz_os_wide(const char *utf8, const wchar_t *suffix, int64_t *status) {
    return tz_os_wide_ex(utf8, suffix, 1, status);
}

static char *tz_os_wide_to_utf8(const wchar_t *wide, size_t wlen, size_t *out_len, int64_t *status) {
    if (wlen == 0) {
        char *empty = (char *)malloc(1);
        if (empty == NULL) {
            *status = tz_os_status(TZ_OS_OTHER, ERROR_NOT_ENOUGH_MEMORY);
            return NULL;
        }
        empty[0] = '\0';
        if (out_len != NULL) *out_len = 0;
        return empty;
    }
    if (wlen > (size_t)INT_MAX) {
        *status = tz_os_status(TZ_OS_OTHER, ERROR_NOT_ENOUGH_MEMORY);
        return NULL;
    }
    int needed = WideCharToMultiByte(CP_UTF8, WC_ERR_INVALID_CHARS, wide, (int)wlen, NULL, 0, NULL, NULL);
    if (needed <= 0) {
        *status = tz_os_win_error(GetLastError());
        return NULL;
    }
    char *utf8 = (char *)malloc((size_t)needed + 1);
    if (utf8 == NULL) {
        *status = tz_os_status(TZ_OS_OTHER, ERROR_NOT_ENOUGH_MEMORY);
        return NULL;
    }
    if (WideCharToMultiByte(CP_UTF8, WC_ERR_INVALID_CHARS, wide, (int)wlen, utf8, needed, NULL, NULL) <= 0) {
        DWORD err = GetLastError();
        free(utf8);
        *status = tz_os_win_error(err);
        return NULL;
    }
    utf8[needed] = '\0';
    if (out_len != NULL) *out_len = (size_t)needed;
    return utf8;
}

static int64_t tz_os_narrow(const wchar_t *wide, size_t wlen, struct tz_os_buffer *output) {
    tz_os_clear(output);
    if (wlen == 0) return 0;
    size_t utf8_len = 0;
    int64_t status = 0;
    char *utf8 = tz_os_wide_to_utf8(wide, wlen, &utf8_len, &status);
    if (utf8 == NULL) return status;
    status = tz_os_deliver(output, (const unsigned char *)utf8, utf8_len);
    free(utf8);
    return status;
}

static HANDLE tz_os_open_file(const wchar_t *path, DWORD access, DWORD disposition, DWORD flags) {
    return CreateFileW(
        path,
        access,
        FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
        NULL,
        disposition,
        flags,
        NULL
    );
}

static int tz_os_is_directory_path(const wchar_t *path) {
    DWORD attrs = GetFileAttributesW(path);
    return attrs != INVALID_FILE_ATTRIBUTES && (attrs & FILE_ATTRIBUTE_DIRECTORY) != 0;
}

static int tz_os_is_name_surrogate_dir(const wchar_t *path) {
    HANDLE h = tz_os_open_file(
        path,
        FILE_READ_ATTRIBUTES,
        OPEN_EXISTING,
        FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT
    );
    if (h == INVALID_HANDLE_VALUE) return 0;
    FILE_ATTRIBUTE_TAG_INFO info;
    int result = 0;
    if (GetFileInformationByHandleEx(h, FileAttributeTagInfo, &info, sizeof(info))) {
        if ((info.FileAttributes & FILE_ATTRIBUTE_REPARSE_POINT) != 0 &&
            (info.FileAttributes & FILE_ATTRIBUTE_DIRECTORY) != 0 &&
            (info.ReparseTag & 0x20000000u) != 0) {
            result = 1;
        }
    }
    CloseHandle(h);
    return result;
}

static int64_t tz_os_read_file(struct tz_os_buffer *output, const char *name) {
    int64_t status = 0;
    wchar_t *wpath = tz_os_wide(name, NULL, &status);
    if (wpath == NULL) return status;
    HANDLE h = tz_os_open_file(wpath, GENERIC_READ, OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL);
    if (h == INVALID_HANDLE_VALUE) {
        DWORD err = GetLastError();
        int is_dir = (err == ERROR_ACCESS_DENIED) && tz_os_is_directory_path(wpath);
        free(wpath);
        return is_dir ? tz_os_invalid_input() : tz_os_win_error(err);
    }
    free(wpath);
    BY_HANDLE_FILE_INFORMATION info;
    if (GetFileInformationByHandle(h, &info) && (info.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY) != 0) {
        CloseHandle(h);
        return tz_os_invalid_input();
    }
    LARGE_INTEGER size;
    size_t capacity = 4096;
    if (GetFileSizeEx(h, &size) && size.QuadPart > 0 && (uint64_t)size.QuadPart < (uint64_t)SIZE_MAX / 2) {
        capacity = (size_t)size.QuadPart + 1;
    }
    unsigned char *buffer = (unsigned char *)malloc(capacity);
    if (buffer == NULL) {
        CloseHandle(h);
        return tz_os_status(TZ_OS_OTHER, ERROR_NOT_ENOUGH_MEMORY);
    }
    size_t length = 0;
    for (;;) {
        if (length == capacity) {
            if (capacity > (size_t)INT64_MAX / 2) {
                free(buffer);
                CloseHandle(h);
                return tz_os_status(TZ_OS_OTHER, ERROR_NOT_ENOUGH_MEMORY);
            }
            unsigned char *larger = (unsigned char *)realloc(buffer, capacity * 2);
            if (larger == NULL) {
                free(buffer);
                CloseHandle(h);
                return tz_os_status(TZ_OS_OTHER, ERROR_NOT_ENOUGH_MEMORY);
            }
            buffer = larger;
            capacity *= 2;
        }
        size_t avail = capacity - length;
        DWORD chunk = avail > (size_t)(1u << 30) ? (DWORD)(1u << 30) : (DWORD)avail;
        DWORD got = 0;
        if (!ReadFile(h, buffer + length, chunk, &got, NULL)) {
            DWORD err = GetLastError();
            if (err == ERROR_BROKEN_PIPE) break;
            free(buffer);
            CloseHandle(h);
            return tz_os_win_error(err);
        }
        if (got == 0) break;
        length += (size_t)got;
    }
    CloseHandle(h);
    status = tz_os_deliver(output, buffer, length);
    free(buffer);
    return status;
}

static int64_t tz_os_list_directory(struct tz_os_buffer *output, const char *name) {
    if (name[0] == '\0') return tz_os_win_error(ERROR_PATH_NOT_FOUND);
    int64_t status = 0;
    wchar_t *wbase = tz_os_wide(name, NULL, &status);
    if (wbase == NULL) return status;
    wchar_t *pattern = tz_os_wide(name, L"\\*", &status);
    if (pattern == NULL) {
        free(wbase);
        return status;
    }
    WIN32_FIND_DATAW entry;
    HANDLE find = FindFirstFileExW(pattern, FindExInfoBasic, &entry, FindExSearchNameMatch, NULL, FIND_FIRST_EX_LARGE_FETCH);
    free(pattern);
    if (find == INVALID_HANDLE_VALUE) {
        DWORD err = GetLastError();
        int is_dir = (err == ERROR_FILE_NOT_FOUND) && tz_os_is_directory_path(wbase);
        free(wbase);
        return is_dir ? 0 : tz_os_win_error(err);
    }
    free(wbase);

    char **names = NULL;
    size_t count = 0, capacity = 0, total = 0;
    do {
        if (wcscmp(entry.cFileName, L".") == 0 || wcscmp(entry.cFileName, L"..") == 0) continue;
        if (count == capacity) {
            size_t next = capacity == 0 ? 16 : capacity * 2;
            char **larger = (char **)realloc(names, next * sizeof(char *));
            if (larger == NULL) {
                status = tz_os_status(TZ_OS_OTHER, ERROR_NOT_ENOUGH_MEMORY);
                break;
            }
            names = larger;
            capacity = next;
        }
        size_t utf8_len = 0;
        char *copy = tz_os_wide_to_utf8(entry.cFileName, wcslen(entry.cFileName), &utf8_len, &status);
        if (copy == NULL) break;
        names[count++] = copy;
        total += utf8_len + 1;
    } while (FindNextFileW(find, &entry));
    if (status == 0) {
        DWORD err = GetLastError();
        if (err != ERROR_NO_MORE_FILES) status = tz_os_win_error(err);
    }
    FindClose(find);

    if (status == 0) {
        qsort(names, count, sizeof(char *), tz_os_compare_names);
        unsigned char *joined = total == 0 ? NULL : (unsigned char *)malloc(total);
        if (total != 0 && joined == NULL) {
            status = tz_os_status(TZ_OS_OTHER, ERROR_NOT_ENOUGH_MEMORY);
        } else {
            size_t offset = 0;
            for (size_t index = 0; index < count; index++) {
                size_t len = strlen(names[index]) + 1;
                memcpy(joined + offset, names[index], len);
                offset += len;
            }
            status = tz_os_deliver(output, joined, total);
            free(joined);
        }
    }
    for (size_t index = 0; index < count; index++) free(names[index]);
    free(names);
    return status;
}

static int64_t tz_os_current_directory(struct tz_os_buffer *output) {
    DWORD cap = 4096;
    for (;;) {
        wchar_t *buffer = (wchar_t *)malloc((size_t)cap * sizeof(wchar_t));
        if (buffer == NULL) return tz_os_status(TZ_OS_OTHER, ERROR_NOT_ENOUGH_MEMORY);
        DWORD got = GetCurrentDirectoryW(cap, buffer);
        if (got == 0) {
            DWORD err = GetLastError();
            free(buffer);
            return tz_os_win_error(err);
        }
        if (got < cap) {
            int64_t status = tz_os_narrow(buffer, (size_t)got, output);
            free(buffer);
            return status;
        }
        free(buffer);
        if (got > TZ_OS_MAX_CURRENT_DIR) return tz_os_status(TZ_OS_OTHER, ERROR_INSUFFICIENT_BUFFER);
        cap = got;
    }
}

static int64_t tz_os_filetime_to_unix_ns(FILETIME ft) {
    uint64_t ticks = ((uint64_t)ft.dwHighDateTime << 32) | (uint64_t)ft.dwLowDateTime;
    const uint64_t epoch = UINT64_C(116444736000000000);
    if (ticks >= epoch) {
        uint64_t diff = ticks - epoch;
        if (diff > (uint64_t)INT64_MAX / 100) return INT64_MAX;
        return (int64_t)(diff * 100);
    } else {
        uint64_t diff = epoch - ticks;
        if (diff > (uint64_t)INT64_MAX / 100) return INT64_MIN;
        return -(int64_t)(diff * 100);
    }
}

static int64_t tz_os_stat(struct tz_os_buffer *output, const char *name, int follow) {
    int64_t status = 0;
    wchar_t *wpath = tz_os_wide(name, NULL, &status);
    if (wpath == NULL) return status;
    DWORD flags = FILE_FLAG_BACKUP_SEMANTICS | (follow ? 0u : FILE_FLAG_OPEN_REPARSE_POINT);
    HANDLE h = tz_os_open_file(wpath, FILE_READ_ATTRIBUTES, OPEN_EXISTING, flags);
    free(wpath);
    if (h == INVALID_HANDLE_VALUE) return tz_os_win_error(GetLastError());

    BY_HANDLE_FILE_INFORMATION info;
    if (!GetFileInformationByHandle(h, &info)) {
        DWORD err = GetLastError();
        DWORD ftype = GetFileType(h);
        CloseHandle(h);
        if (ftype != FILE_TYPE_UNKNOWN && ftype != FILE_TYPE_DISK) {
            unsigned char record[24];
            memset(record, 0, sizeof record);
            record[0] = 3;
            return tz_os_deliver(output, record, sizeof record);
        }
        return tz_os_win_error(err);
    }

    int is_link = 0;
    if (!follow && (info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT) != 0) {
        FILE_ATTRIBUTE_TAG_INFO tag;
        if (GetFileInformationByHandleEx(h, FileAttributeTagInfo, &tag, sizeof(tag)) &&
            (tag.ReparseTag & 0x20000000u) != 0) {
            is_link = 1;
        }
    }
    DWORD ftype = GetFileType(h);
    CloseHandle(h);

    unsigned char record[24];
    memset(record, 0, sizeof record);
    unsigned char kind;
    if (is_link) {
        kind = 2;
    } else if (ftype != FILE_TYPE_DISK) {
        kind = 3;
    } else if ((info.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY) != 0) {
        kind = 1;
    } else {
        kind = 0;
    }
    record[0] = kind;
    int64_t file_size = (kind == 1)
        ? 0
        : (int64_t)(((uint64_t)info.nFileSizeHigh << 32) | (uint64_t)info.nFileSizeLow);
    tz_os_put_i64(record + 8, file_size);
    tz_os_put_i64(record + 16, tz_os_filetime_to_unix_ns(info.ftLastWriteTime));
    return tz_os_deliver(output, record, sizeof record);
}

static int64_t tz_os_environment(struct tz_os_buffer *output, const char *name) {
    if (name[0] == '\0' || strchr(name, '=') != NULL) return tz_os_invalid_input();
    int64_t status = 0;
    wchar_t *wname = tz_os_wide_ex(name, NULL, 0, &status);
    if (wname == NULL) return status;
    DWORD cap = 256;
    for (;;) {
        wchar_t *buf = (wchar_t *)malloc((size_t)cap * sizeof(wchar_t));
        if (buf == NULL) {
            free(wname);
            return tz_os_status(TZ_OS_OTHER, ERROR_NOT_ENOUGH_MEMORY);
        }
        SetLastError(0);
        DWORD got = GetEnvironmentVariableW(wname, buf, cap);
        if (got == 0) {
            DWORD err = GetLastError();
            free(buf);
            free(wname);
            if (err == ERROR_ENVVAR_NOT_FOUND) return tz_os_status(TZ_OS_NOT_FOUND, 0);
            if (err == 0) return 0; // Present with empty value.
            return tz_os_win_error(err);
        }
        if (got < cap) {
            status = tz_os_narrow(buf, (size_t)got, output);
            free(buf);
            free(wname);
            return status;
        }
        free(buf);
        cap = got;
    }
}

static int64_t tz_os_read_now(struct tz_os_buffer *output, int32_t operation, const unsigned char *path, int64_t length) {
    tz_os_clear(output);
    if (operation == 2) return tz_os_current_directory(output);
    int64_t status = 0;
    char *name = tz_os_name(path, length, &status);
    if (name == NULL) return status;
    switch (operation) {
    case 0:
        status = tz_os_read_file(output, name);
        break;
    case 1:
        status = tz_os_list_directory(output, name);
        break;
    case 3:
        status = tz_os_environment(output, name);
        break;
    case 4:
    case 5:
        status = tz_os_stat(output, name, operation == 4);
        break;
    default:
        status = tz_os_invalid_input();
        break;
    }
    free(name);
    return status;
}

TZ_OS_API int64_t tsuzuri_os_read(struct tz_os_buffer *output, int32_t operation, const unsigned char *path, int64_t length) {
    TZ_BLOCKING_ENTER();
    int64_t status = tz_os_read_now(output, operation, path, length);
    TZ_BLOCKING_EXIT();
    return status;
}

static int tz_os_argument_count = 0;
static char **tz_os_arguments = NULL;

TZ_OS_API void tsuzuri_os_set_args(int32_t count, char **arguments) {
    tz_os_argument_count = count;
    tz_os_arguments = arguments;
}

TZ_OS_API int64_t tsuzuri_os_args(struct tz_os_buffer *output) {
    tz_os_clear(output);
    if (tz_os_argument_count == 0) return 0;
    const uint16_t *line = (const uint16_t *)GetCommandLineW();
    if (line == NULL) return 0;
    int64_t length = 0;
    const uint16_t *start = tz_arguments_blank(line);
    if (*start != 0) start = tz_arguments_blank(tz_arguments_token(start, NULL, &length));
    size_t arg_count = 0;
    for (const uint16_t *at = start; *at != 0; arg_count++) {
        at = tz_arguments_blank(tz_arguments_token(at, NULL, &length));
    }
    if (arg_count == 0) return 0;

    char **items = (char **)calloc(arg_count, sizeof(char *));
    size_t *lengths = (size_t *)calloc(arg_count, sizeof(size_t));
    if (items == NULL || lengths == NULL) {
        free(items);
        free(lengths);
        return tz_os_status(TZ_OS_OTHER, ERROR_NOT_ENOUGH_MEMORY);
    }
    int64_t status = 0;
    size_t total = 0;
    const uint16_t *at = start;
    for (size_t index = 0; index < arg_count; index++) {
        tz_arguments_token(at, NULL, &length);
        uint16_t *units = length > 0 ? (uint16_t *)malloc((size_t)length * sizeof(uint16_t)) : NULL;
        if (length > 0 && units == NULL) {
            status = tz_os_status(TZ_OS_OTHER, ERROR_NOT_ENOUGH_MEMORY);
            break;
        }
        at = tz_arguments_blank(tz_arguments_token(at, units, &length));
        size_t utf8_len = 0;
        char *utf8 = tz_os_wide_to_utf8((const wchar_t *)units, (size_t)length, &utf8_len, &status);
        free(units);
        if (utf8 == NULL) break;
        items[index] = utf8;
        lengths[index] = utf8_len;
        total += utf8_len + 1;
    }
    if (status == 0) {
        unsigned char *joined = (unsigned char *)malloc(total);
        if (joined == NULL) {
            status = tz_os_status(TZ_OS_OTHER, ERROR_NOT_ENOUGH_MEMORY);
        } else {
            size_t offset = 0;
            for (size_t index = 0; index < arg_count; index++) {
                if (lengths[index] > 0) memcpy(joined + offset, items[index], lengths[index]);
                joined[offset + lengths[index]] = 0;
                offset += lengths[index] + 1;
            }
            status = tz_os_deliver(output, joined, total);
            free(joined);
        }
    }
    for (size_t index = 0; index < arg_count; index++) free(items[index]);
    free(items);
    free(lengths);
    return status;
}

static int64_t tz_os_write_all(HANDLE h, const unsigned char *data, int64_t length) {
    int64_t offset = 0;
    while (offset < length) {
        int64_t rem = length - offset;
        DWORD chunk = rem > (INT64_C(1) << 30) ? (DWORD)(1u << 30) : (DWORD)rem;
        DWORD wrote = 0;
        if (!WriteFile(h, data + offset, chunk, &wrote, NULL)) {
            return tz_os_win_error(GetLastError());
        }
        offset += (int64_t)wrote;
    }
    return 0;
}

static int64_t tz_os_write_file(const char *name, DWORD access, DWORD disposition, const unsigned char *data, int64_t length) {
    int64_t status = 0;
    wchar_t *wpath = tz_os_wide(name, NULL, &status);
    if (wpath == NULL) return status;
    HANDLE h = tz_os_open_file(wpath, access, disposition, FILE_ATTRIBUTE_NORMAL);
    if (h == INVALID_HANDLE_VALUE) {
        DWORD err = GetLastError();
        int is_dir = (err == ERROR_ACCESS_DENIED) && tz_os_is_directory_path(wpath);
        free(wpath);
        return is_dir ? tz_os_invalid_input() : tz_os_win_error(err);
    }
    free(wpath);
    status = tz_os_write_all(h, data, length);
    if (!CloseHandle(h) && status == 0) status = tz_os_win_error(GetLastError());
    return status;
}

static int64_t tz_os_write_now(int32_t operation, const unsigned char *path, int64_t path_length, const unsigned char *data, int64_t data_length) {
    int64_t status = 0;
    char *name = tz_os_name(path, path_length, &status);
    if (name == NULL) return status;
    switch (operation) {
    case 0:
        status = tz_os_write_file(name, GENERIC_WRITE, CREATE_ALWAYS, data, data_length);
        break;
    case 1:
        status = tz_os_write_file(name, FILE_APPEND_DATA, OPEN_ALWAYS, data, data_length);
        break;
    case 2: {
        wchar_t *wpath = tz_os_wide(name, NULL, &status);
        if (wpath != NULL) {
            if (!CreateDirectoryW(wpath, NULL)) status = tz_os_win_error(GetLastError());
            free(wpath);
        }
        break;
    }
    case 3: {
        wchar_t *wpath = tz_os_wide(name, NULL, &status);
        if (wpath != NULL) {
            if (tz_os_is_name_surrogate_dir(wpath)) {
                status = tz_os_status(TZ_OS_NOT_FOUND, 0);
            } else if (!RemoveDirectoryW(wpath)) {
                status = tz_os_win_error(GetLastError());
            }
            free(wpath);
        }
        break;
    }
    case 4: {
        wchar_t *wpath = tz_os_wide(name, NULL, &status);
        if (wpath != NULL) {
            if (tz_os_is_name_surrogate_dir(wpath)) {
                if (!RemoveDirectoryW(wpath)) status = tz_os_win_error(GetLastError());
            } else if (!DeleteFileW(wpath)) {
                DWORD err = GetLastError();
                int is_dir = (err == ERROR_ACCESS_DENIED) && tz_os_is_directory_path(wpath);
                status = is_dir ? tz_os_invalid_input() : tz_os_win_error(err);
            }
            free(wpath);
        }
        break;
    }
    default:
        status = tz_os_invalid_input();
        break;
    }
    free(name);
    return status;
}

TZ_OS_API int64_t tsuzuri_os_write(int32_t operation, const unsigned char *path, int64_t path_length, const unsigned char *data, int64_t data_length) {
    TZ_BLOCKING_ENTER();
    int64_t status = tz_os_write_now(operation, path, path_length, data, data_length);
    TZ_BLOCKING_EXIT();
    return status;
}

struct tz_os_file {
    HANDLE handle;
    int64_t generation;
    int readable;
    int writable;
};

static struct tz_os_file *tz_os_files = NULL;
static size_t tz_os_file_capacity = 0;
static size_t tz_os_files_open = 0;
static int64_t tz_os_generation = 0;
static SRWLOCK tz_os_files_lock = SRWLOCK_INIT;

static struct tz_os_file *tz_os_file_of(int64_t handle) {
    if (handle <= 0) return NULL;
    uint64_t slot = (uint64_t)handle & UINT64_C(0xffffffff);
    int64_t generation = handle >> 32;
    if (slot == 0 || slot > tz_os_file_capacity) return NULL;
    struct tz_os_file *file = &tz_os_files[slot - 1];
    return file->generation == generation && generation != 0 ? file : NULL;
}

static int64_t tz_os_open_now(int32_t mode, const unsigned char *path, int64_t length) {
    int64_t status = 0;
    char *name = tz_os_name(path, length, &status);
    if (name == NULL) return -status;
    DWORD access = 0, disposition = 0;
    int readable = 0, writable = 1;
    switch (mode) {
    case 0:
        access = GENERIC_READ;
        disposition = OPEN_EXISTING;
        readable = 1;
        writable = 0;
        break;
    case 1:
        access = GENERIC_WRITE;
        disposition = CREATE_ALWAYS;
        break;
    case 2:
        access = FILE_APPEND_DATA;
        disposition = OPEN_ALWAYS;
        break;
    case 3:
        access = GENERIC_WRITE;
        disposition = CREATE_NEW;
        break;
    default:
        free(name);
        return -tz_os_invalid_input();
    }
    wchar_t *wpath = tz_os_wide(name, NULL, &status);
    free(name);
    if (wpath == NULL) return -status;
    HANDLE os_handle = tz_os_open_file(wpath, access, disposition, FILE_ATTRIBUTE_NORMAL);
    if (os_handle == INVALID_HANDLE_VALUE) {
        DWORD err = GetLastError();
        int is_dir = (err == ERROR_ACCESS_DENIED) && tz_os_is_directory_path(wpath);
        free(wpath);
        return -(is_dir ? tz_os_invalid_input() : tz_os_win_error(err));
    }
    free(wpath);
    BY_HANDLE_FILE_INFORMATION info;
    if (GetFileInformationByHandle(os_handle, &info) && (info.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY) != 0) {
        CloseHandle(os_handle);
        return -tz_os_invalid_input();
    }

    AcquireSRWLockExclusive(&tz_os_files_lock);
    int64_t failure = 0;
    if (tz_os_generation >= INT32_MAX) failure = tz_os_status(TZ_OS_OTHER, ERROR_TOO_MANY_OPEN_FILES);
    size_t slot = 0;
    while (failure == 0 && slot < tz_os_file_capacity && tz_os_files[slot].generation != 0) slot++;
    if (failure == 0 && slot == tz_os_file_capacity) {
        size_t next = tz_os_file_capacity == 0 ? 8 : tz_os_file_capacity * 2;
        struct tz_os_file *larger = next > UINT32_MAX ? NULL : (struct tz_os_file *)realloc(tz_os_files, next * sizeof(struct tz_os_file));
        if (larger == NULL) {
            failure = tz_os_status(TZ_OS_OTHER, next > UINT32_MAX ? ERROR_TOO_MANY_OPEN_FILES : ERROR_NOT_ENOUGH_MEMORY);
        } else {
            for (size_t index = tz_os_file_capacity; index < next; index++) larger[index].generation = 0;
            tz_os_files = larger;
            tz_os_file_capacity = next;
        }
    }
    int64_t handle = 0;
    if (failure == 0) {
        struct tz_os_file *file = &tz_os_files[slot];
        file->handle = os_handle;
        file->generation = ++tz_os_generation;
        file->readable = readable;
        file->writable = writable;
        tz_os_files_open++;
        handle = (file->generation << 32) | (int64_t)(slot + 1);
    }
    ReleaseSRWLockExclusive(&tz_os_files_lock);
    if (failure != 0) {
        CloseHandle(os_handle);
        return -failure;
    }
    return handle;
}

TZ_OS_API int64_t tsuzuri_os_open(int32_t mode, const unsigned char *path, int64_t length) {
    TZ_BLOCKING_ENTER();
    int64_t status = tz_os_open_now(mode, path, length);
    TZ_BLOCKING_EXIT();
    return status;
}

static int64_t tz_os_handle_now(struct tz_os_buffer *output, int32_t operation, int64_t handle, int64_t count, const unsigned char *data, int64_t data_length) {
    tz_os_clear(output);
    AcquireSRWLockExclusive(&tz_os_files_lock);
    struct tz_os_file *entry = tz_os_file_of(handle);
    struct tz_os_file copy;
    if (entry != NULL) copy = *entry;
    ReleaseSRWLockExclusive(&tz_os_files_lock);
    if (entry == NULL) return tz_os_bad_handle();
    const struct tz_os_file *file = &copy;
    switch (operation) {
    case 0: {
        if (!file->readable) return tz_os_bad_handle();
        if (count < 0 || count > TZ_OS_MAX_RANDOM) return tz_os_invalid_input();
        if (count == 0) return 0;
        unsigned char *bytes = (unsigned char *)tsuzuri_alloc(count);
        DWORD got = 0;
        if (!ReadFile(file->handle, bytes, (DWORD)count, &got, NULL)) {
            DWORD err = GetLastError();
            tsuzuri_free(bytes);
            if (err == ERROR_BROKEN_PIPE) return 0;
            return tz_os_win_error(err);
        }
        if (got == 0) {
            tsuzuri_free(bytes);
            return 0;
        }
        output->data = bytes;
        output->length = (int64_t)got;
        return 0;
    }
    case 1: {
        if (!file->writable) return tz_os_bad_handle();
        return tz_os_write_all(file->handle, data, data_length);
    }
    case 2:
        return 0;
    default:
        return tz_os_invalid_input();
    }
}

TZ_OS_API int64_t tsuzuri_os_handle(struct tz_os_buffer *output, int32_t operation, int64_t handle, int64_t count, const unsigned char *data, int64_t data_length) {
    TZ_BLOCKING_ENTER();
    int64_t status = tz_os_handle_now(output, operation, handle, count, data, data_length);
    TZ_BLOCKING_EXIT();
    return status;
}

static int64_t tz_os_close_now(int64_t handle) {
    AcquireSRWLockExclusive(&tz_os_files_lock);
    struct tz_os_file *file = tz_os_file_of(handle);
    HANDLE os_handle = file != NULL ? file->handle : INVALID_HANDLE_VALUE;
    if (file != NULL) {
        file->generation = 0;
        if (--tz_os_files_open == 0) {
            free(tz_os_files);
            tz_os_files = NULL;
            tz_os_file_capacity = 0;
        }
    }
    ReleaseSRWLockExclusive(&tz_os_files_lock);
    if (os_handle == INVALID_HANDLE_VALUE) return tz_os_bad_handle();
    if (!CloseHandle(os_handle)) return tz_os_win_error(GetLastError());
    return 0;
}

TZ_OS_API int64_t tsuzuri_os_close(int64_t handle) {
    TZ_BLOCKING_ENTER();
    int64_t status = tz_os_close_now(handle);
    TZ_BLOCKING_EXIT();
    return status;
}

/* ---------------- Phase 2: Process.run (CreateProcessW + overlapped named pipes) ---------------- */

static int tz_os_is_batch_path(const wchar_t *path) {
    const wchar_t *base = path;
    for (const wchar_t *p = path; *p != 0; p++) {
        if (*p == L'\\' || *p == L'/') base = p + 1;
    }
    size_t len = wcslen(base);
    // Strip alternate data stream suffix (`:stream`).
    for (size_t i = 0; i < len; i++) {
        if (base[i] == L':') {
            len = i;
            break;
        }
    }
    // Strip trailing dots and spaces (Win32 path normalization, CVE-2024-43402).
    while (len > 0 && (base[len - 1] == L'.' || base[len - 1] == L' ' || base[len - 1] == L'\t')) {
        len--;
    }
    if (len < 4) return 0;
    const wchar_t *ext = base + len - 4;
    if (ext[0] != L'.') return 0;
    wchar_t c1 = ext[1], c2 = ext[2], c3 = ext[3];
    if (c1 >= L'A' && c1 <= L'Z') c1 = (wchar_t)(c1 - L'A' + L'a');
    if (c2 >= L'A' && c2 <= L'Z') c2 = (wchar_t)(c2 - L'A' + L'a');
    if (c3 >= L'A' && c3 <= L'Z') c3 = (wchar_t)(c3 - L'A' + L'a');
    return (c1 == L'b' && c2 == L'a' && c3 == L't') || (c1 == L'c' && c2 == L'm' && c3 == L'd');
}

static int tz_os_file_exists_non_dir(const wchar_t *path) {
    DWORD attrs = GetFileAttributesW(path);
    return attrs != INVALID_FILE_ATTRIBUTES && (attrs & FILE_ATTRIBUTE_DIRECTORY) == 0;
}

static wchar_t *tz_os_full_path_alloc(const wchar_t *path, int64_t *status) {
    DWORD need = GetFullPathNameW(path, 0, NULL, NULL);
    if (need == 0) {
        *status = tz_os_win_error(GetLastError());
        return NULL;
    }
    wchar_t *full = (wchar_t *)malloc((size_t)need * sizeof(wchar_t));
    if (full == NULL) {
        *status = tz_os_status(TZ_OS_OTHER, ERROR_NOT_ENOUGH_MEMORY);
        return NULL;
    }
    DWORD got = GetFullPathNameW(path, need, full, NULL);
    if (got == 0 || got >= need) {
        DWORD err = got == 0 ? GetLastError() : ERROR_INSUFFICIENT_BUFFER;
        free(full);
        *status = tz_os_win_error(err);
        return NULL;
    }
    return full;
}

static wchar_t *tz_os_join_dir_prog(const wchar_t *dir, size_t dir_len, const wchar_t *prog, const wchar_t *ext, int64_t *status) {
    size_t prog_len = wcslen(prog);
    size_t ext_len = ext != NULL ? wcslen(ext) : 0;
    int need_sep = dir_len > 0 && dir[dir_len - 1] != L'\\' && dir[dir_len - 1] != L'/';
    size_t total = dir_len + (size_t)need_sep + prog_len + ext_len;
    wchar_t *out = (wchar_t *)malloc((total + 1) * sizeof(wchar_t));
    if (out == NULL) {
        *status = tz_os_status(TZ_OS_OTHER, ERROR_NOT_ENOUGH_MEMORY);
        return NULL;
    }
    size_t pos = 0;
    if (dir_len > 0) {
        memcpy(out, dir, dir_len * sizeof(wchar_t));
        pos += dir_len;
    }
    if (need_sep) out[pos++] = L'\\';
    memcpy(out + pos, prog, prog_len * sizeof(wchar_t));
    pos += prog_len;
    if (ext_len > 0) {
        memcpy(out + pos, ext, ext_len * sizeof(wchar_t));
        pos += ext_len;
    }
    out[pos] = 0;
    return out;
}

static wchar_t *tz_os_resolve_program(const wchar_t *wprog, int64_t *status) {
    if (tz_os_is_batch_path(wprog)) {
        *status = tz_os_invalid_input();
        return NULL;
    }
    int has_sep = 0;
    const wchar_t *base = wprog;
    for (const wchar_t *p = wprog; *p != 0; p++) {
        if (*p == L'/' || *p == L'\\' || *p == L':') {
            has_sep = 1;
            if (*p == L'/' || *p == L'\\') base = p + 1;
        }
    }
    int has_dot = wcschr(base, L'.') != NULL;
    const wchar_t *exts[2];
    if (has_dot) {
        exts[0] = L"";
        exts[1] = L".exe";
    } else {
        exts[0] = L".exe";
        exts[1] = L".com";
    }

    if (has_sep) {
        for (int i = 0; i < 2; i++) {
            wchar_t *cand = tz_os_join_dir_prog(L"", 0, wprog, exts[i], status);
            if (cand == NULL) return NULL;
            if (tz_os_file_exists_non_dir(cand)) {
                wchar_t *full = tz_os_full_path_alloc(cand, status);
                free(cand);
                if (full == NULL) return NULL;
                if (tz_os_is_batch_path(full)) {
                    free(full);
                    *status = tz_os_invalid_input();
                    return NULL;
                }
                return full;
            }
            free(cand);
        }
        *status = tz_os_win_error(ERROR_FILE_NOT_FOUND);
        return NULL;
    }

    // Search PATH only (never CWD or executable directory).
    DWORD cap = 1024;
    wchar_t *path_env = NULL;
    for (;;) {
        path_env = (wchar_t *)malloc((size_t)cap * sizeof(wchar_t));
        if (path_env == NULL) {
            *status = tz_os_status(TZ_OS_OTHER, ERROR_NOT_ENOUGH_MEMORY);
            return NULL;
        }
        SetLastError(0);
        DWORD got = GetEnvironmentVariableW(L"PATH", path_env, cap);
        if (got == 0) {
            free(path_env);
            *status = tz_os_win_error(ERROR_FILE_NOT_FOUND);
            return NULL;
        }
        if (got < cap) break;
        free(path_env);
        cap = got;
    }

    const wchar_t *cursor = path_env;
    while (*cursor != 0) {
        const wchar_t *semi = wcschr(cursor, L';');
        const wchar_t *end = semi != NULL ? semi : cursor + wcslen(cursor);
        const wchar_t *dstart = cursor;
        const wchar_t *dend = end;
        if (dend - dstart >= 2 && dstart[0] == L'"' && dend[-1] == L'"') {
            dstart++;
            dend--;
        }
        if (dend > dstart) {
            size_t dlen = (size_t)(dend - dstart);
            for (int i = 0; i < 2; i++) {
                wchar_t *cand = tz_os_join_dir_prog(dstart, dlen, wprog, exts[i], status);
                if (cand == NULL) {
                    free(path_env);
                    return NULL;
                }
                if (tz_os_file_exists_non_dir(cand)) {
                    wchar_t *full = tz_os_full_path_alloc(cand, status);
                    free(cand);
                    free(path_env);
                    if (full == NULL) return NULL;
                    if (tz_os_is_batch_path(full)) {
                        free(full);
                        *status = tz_os_invalid_input();
                        return NULL;
                    }
                    return full;
                }
                free(cand);
            }
        }
        if (semi == NULL) break;
        cursor = semi + 1;
    }
    free(path_env);
    *status = tz_os_win_error(ERROR_FILE_NOT_FOUND);
    return NULL;
}

struct tz_os_wbuf {
    wchar_t *data;
    size_t len;
    size_t cap;
};

static int tz_os_wbuf_push(struct tz_os_wbuf *b, wchar_t ch) {
    if (b->len + 1 >= b->cap) {
        size_t next = b->cap == 0 ? 256 : b->cap * 2;
        wchar_t *larger = (wchar_t *)realloc(b->data, next * sizeof(wchar_t));
        if (larger == NULL) return 0;
        b->data = larger;
        b->cap = next;
    }
    b->data[b->len++] = ch;
    b->data[b->len] = 0;
    return 1;
}

static int tz_os_append_quoted_arg(struct tz_os_wbuf *b, const wchar_t *arg) {
    int needs_quotes = arg[0] == 0;
    for (const wchar_t *p = arg; !needs_quotes && *p != 0; p++) {
        if (*p == L' ' || *p == L'\t' || *p == L'\n' || *p == L'\r' || *p == L'\v' || *p == L'"') {
            needs_quotes = 1;
        }
    }
    if (!needs_quotes) {
        for (const wchar_t *p = arg; *p != 0; p++) {
            if (!tz_os_wbuf_push(b, *p)) return 0;
        }
        return 1;
    }
    if (!tz_os_wbuf_push(b, L'"')) return 0;
    size_t backslashes = 0;
    for (const wchar_t *p = arg; *p != 0; p++) {
        if (*p == L'\\') {
            backslashes++;
        } else if (*p == L'"') {
            for (size_t i = 0; i < backslashes * 2 + 1; i++) {
                if (!tz_os_wbuf_push(b, L'\\')) return 0;
            }
            backslashes = 0;
            if (!tz_os_wbuf_push(b, L'"')) return 0;
        } else {
            for (size_t i = 0; i < backslashes; i++) {
                if (!tz_os_wbuf_push(b, L'\\')) return 0;
            }
            backslashes = 0;
            if (!tz_os_wbuf_push(b, *p)) return 0;
        }
    }
    for (size_t i = 0; i < backslashes * 2; i++) {
        if (!tz_os_wbuf_push(b, L'\\')) return 0;
    }
    return tz_os_wbuf_push(b, L'"');
}

static _Atomic uint64_t tz_os_pipe_seq = 0;

static void tz_os_format_u64_hex(wchar_t *dst, uint64_t value) {
    static const wchar_t hex[] = L"0123456789abcdef";
    for (int i = 15; i >= 0; i--) {
        dst[i] = hex[value & 0xf];
        value >>= 4;
    }
    dst[16] = 0;
}

static int tz_os_make_overlapped_pipe(int parent_writes, HANDLE *parent_out, HANDLE *child_out, DWORD *err_out) {
    *parent_out = INVALID_HANDLE_VALUE;
    *child_out = INVALID_HANDLE_VALUE;
    DWORD pid = GetCurrentProcessId();
    uint64_t seq = atomic_fetch_add_explicit(&tz_os_pipe_seq, 1, memory_order_relaxed);
    uint64_t rnd = 0;
    (void)BCryptGenRandom(NULL, (PUCHAR)&rnd, (ULONG)sizeof(rnd), BCRYPT_USE_SYSTEM_PREFERRED_RNG);

    wchar_t name[128];
    const wchar_t *prefix = L"\\\\.\\pipe\\tsuzuri-";
    size_t pos = wcslen(prefix);
    memcpy(name, prefix, pos * sizeof(wchar_t));
    tz_os_format_u64_hex(name + pos, (uint64_t)pid);
    pos += 16;
    name[pos++] = L'-';
    tz_os_format_u64_hex(name + pos, seq);
    pos += 16;
    name[pos++] = L'-';
    tz_os_format_u64_hex(name + pos, rnd);

    DWORD open_mode = (parent_writes ? PIPE_ACCESS_OUTBOUND : PIPE_ACCESS_INBOUND)
        | FILE_FLAG_OVERLAPPED | FILE_FLAG_FIRST_PIPE_INSTANCE;
    HANDLE server = CreateNamedPipeW(
        name,
        open_mode,
        PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
        1,
        65536,
        65536,
        0,
        NULL
    );
    if (server == INVALID_HANDLE_VALUE) {
        *err_out = GetLastError();
        return 0;
    }
    SECURITY_ATTRIBUTES sa;
    memset(&sa, 0, sizeof sa);
    sa.nLength = sizeof sa;
    sa.bInheritHandle = TRUE;
    HANDLE client = CreateFileW(
        name,
        parent_writes ? GENERIC_READ : GENERIC_WRITE,
        0,
        &sa,
        OPEN_EXISTING,
        FILE_ATTRIBUTE_NORMAL,
        NULL
    );
    if (client == INVALID_HANDLE_VALUE) {
        *err_out = GetLastError();
        CloseHandle(server);
        return 0;
    }
    *parent_out = server;
    *child_out = client;
    return 1;
}

struct tz_os_win_stream {
    HANDLE handle;
    HANDLE event;
    OVERLAPPED ov;
    int pending;
    unsigned char *data;
    size_t length;
    size_t capacity;
    unsigned char chunk[16384];
};

static int tz_os_win_stream_append(struct tz_os_win_stream *s, const unsigned char *src, size_t n) {
    if (n == 0) return 1;
    if (s->length + n > s->capacity) {
        size_t next = s->capacity == 0 ? 4096 : s->capacity * 2;
        while (next < s->length + n) next *= 2;
        unsigned char *larger = (unsigned char *)realloc(s->data, next);
        if (larger == NULL) return 0;
        s->data = larger;
        s->capacity = next;
    }
    memcpy(s->data + s->length, src, n);
    s->length += n;
    return 1;
}

static void tz_os_cancel_and_close(HANDLE *h, OVERLAPPED *ov, int *pending) {
    if (*h != INVALID_HANDLE_VALUE) {
        if (*pending) {
            (void)CancelIoEx(*h, ov);
            DWORD dummy = 0;
            (void)GetOverlappedResult(*h, ov, &dummy, TRUE);
            *pending = 0;
        }
        CloseHandle(*h);
        *h = INVALID_HANDLE_VALUE;
    }
}

static int64_t tz_os_spawn_now(struct tz_os_buffer *output, const unsigned char *program, int64_t program_length, const unsigned char *arguments, int64_t arguments_length, const unsigned char *input, int64_t input_length) {
    tz_os_clear(output);
    int64_t status = 0;
    char *name = tz_os_name(program, program_length, &status);
    if (name == NULL) return status;
    if (name[0] == '\0' || strchr(name, '"') != NULL || arguments_length < 0 || input_length < 0) {
        free(name);
        return tz_os_invalid_input();
    }
    wchar_t *wprog = tz_os_wide_ex(name, NULL, 0, &status);
    free(name);
    if (wprog == NULL) return status;

    wchar_t *resolved = tz_os_resolve_program(wprog, &status);
    if (resolved == NULL) {
        free(wprog);
        return status;
    }

    struct tz_os_wbuf cmd = {NULL, 0, 0};
    int cmd_ok = tz_os_wbuf_push(&cmd, L'"');
    for (const wchar_t *p = wprog; cmd_ok && *p != 0; p++) cmd_ok = tz_os_wbuf_push(&cmd, *p);
    cmd_ok = cmd_ok && tz_os_wbuf_push(&cmd, L'"');
    free(wprog);

    size_t start = 0;
    for (int64_t index = 0; cmd_ok && index < arguments_length; index++) {
        if (arguments[index] == 0) {
            const char *arg_utf8 = (const char *)(arguments + start);
            wchar_t *warg = tz_os_wide_ex(arg_utf8, NULL, 0, &status);
            if (warg == NULL) {
                free(cmd.data);
                free(resolved);
                return status;
            }
            cmd_ok = tz_os_wbuf_push(&cmd, L' ') && tz_os_append_quoted_arg(&cmd, warg);
            free(warg);
            start = (size_t)index + 1;
        }
    }
    if (!cmd_ok) {
        free(cmd.data);
        free(resolved);
        return tz_os_status(TZ_OS_OTHER, ERROR_NOT_ENOUGH_MEMORY);
    }

    HANDLE p_in = INVALID_HANDLE_VALUE, c_in = INVALID_HANDLE_VALUE;
    HANDLE p_out = INVALID_HANDLE_VALUE, c_out = INVALID_HANDLE_VALUE;
    HANDLE p_err = INVALID_HANDLE_VALUE, c_err = INVALID_HANDLE_VALUE;
    DWORD pipe_err = 0;
    if (!tz_os_make_overlapped_pipe(1, &p_in, &c_in, &pipe_err) ||
        !tz_os_make_overlapped_pipe(0, &p_out, &c_out, &pipe_err) ||
        !tz_os_make_overlapped_pipe(0, &p_err, &c_err, &pipe_err)) {
        if (p_in != INVALID_HANDLE_VALUE) CloseHandle(p_in);
        if (c_in != INVALID_HANDLE_VALUE) CloseHandle(c_in);
        if (p_out != INVALID_HANDLE_VALUE) CloseHandle(p_out);
        if (c_out != INVALID_HANDLE_VALUE) CloseHandle(c_out);
        if (p_err != INVALID_HANDLE_VALUE) CloseHandle(p_err);
        if (c_err != INVALID_HANDLE_VALUE) CloseHandle(c_err);
        free(cmd.data);
        free(resolved);
        return tz_os_win_error(pipe_err);
    }

    SIZE_T attr_size = 0;
    (void)InitializeProcThreadAttributeList(NULL, 1, 0, &attr_size);
    LPPROC_THREAD_ATTRIBUTE_LIST attr_list = (LPPROC_THREAD_ATTRIBUTE_LIST)malloc(attr_size);
    if (attr_list == NULL || !InitializeProcThreadAttributeList(attr_list, 1, 0, &attr_size)) {
        DWORD err = attr_list == NULL ? ERROR_NOT_ENOUGH_MEMORY : GetLastError();
        free(attr_list);
        CloseHandle(p_in); CloseHandle(c_in);
        CloseHandle(p_out); CloseHandle(c_out);
        CloseHandle(p_err); CloseHandle(c_err);
        free(cmd.data);
        free(resolved);
        return tz_os_win_error(err);
    }
    HANDLE inherit_list[3] = {c_in, c_out, c_err};
    if (!UpdateProcThreadAttribute(attr_list, 0, PROC_THREAD_ATTRIBUTE_HANDLE_LIST, inherit_list, sizeof(inherit_list), NULL, NULL)) {
        DWORD err = GetLastError();
        DeleteProcThreadAttributeList(attr_list);
        free(attr_list);
        CloseHandle(p_in); CloseHandle(c_in);
        CloseHandle(p_out); CloseHandle(c_out);
        CloseHandle(p_err); CloseHandle(c_err);
        free(cmd.data);
        free(resolved);
        return tz_os_win_error(err);
    }

    STARTUPINFOEXW si;
    memset(&si, 0, sizeof si);
    si.StartupInfo.cb = sizeof si;
    si.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    si.StartupInfo.hStdInput = c_in;
    si.StartupInfo.hStdOutput = c_out;
    si.StartupInfo.hStdError = c_err;
    si.lpAttributeList = attr_list;

    PROCESS_INFORMATION pi;
    memset(&pi, 0, sizeof pi);
    BOOL created = CreateProcessW(
        resolved,
        cmd.data,
        NULL,
        NULL,
        TRUE,
        CREATE_UNICODE_ENVIRONMENT | EXTENDED_STARTUPINFO_PRESENT,
        NULL,
        NULL,
        &si.StartupInfo,
        &pi
    );
    DWORD create_err = created ? 0 : GetLastError();
    DeleteProcThreadAttributeList(attr_list);
    free(attr_list);
    CloseHandle(c_in);
    CloseHandle(c_out);
    CloseHandle(c_err);
    free(cmd.data);
    free(resolved);
    if (!created) {
        CloseHandle(p_in);
        CloseHandle(p_out);
        CloseHandle(p_err);
        return tz_os_win_error(create_err);
    }

    HANDLE ev_in = CreateEventW(NULL, TRUE, FALSE, NULL);
    HANDLE ev_out = CreateEventW(NULL, TRUE, FALSE, NULL);
    HANDLE ev_err = CreateEventW(NULL, TRUE, FALSE, NULL);
    if (ev_in == NULL || ev_out == NULL || ev_err == NULL) {
        DWORD err = GetLastError();
        if (ev_in != NULL) CloseHandle(ev_in);
        if (ev_out != NULL) CloseHandle(ev_out);
        if (ev_err != NULL) CloseHandle(ev_err);
        CloseHandle(p_in); CloseHandle(p_out); CloseHandle(p_err);
        TerminateProcess(pi.hProcess, 1);
        WaitForSingleObject(pi.hProcess, INFINITE);
        CloseHandle(pi.hThread);
        CloseHandle(pi.hProcess);
        return tz_os_win_error(err);
    }

    OVERLAPPED ov_in;
    memset(&ov_in, 0, sizeof ov_in);
    ov_in.hEvent = ev_in;
    int in_pending = 0;
    size_t written = 0;
    if (input_length == 0) {
        CloseHandle(p_in);
        p_in = INVALID_HANDLE_VALUE;
    }

    struct tz_os_win_stream streams[2];
    memset(streams, 0, sizeof streams);
    streams[0].handle = p_out;
    streams[0].event = ev_out;
    streams[0].ov.hEvent = ev_out;
    streams[1].handle = p_err;
    streams[1].event = ev_err;
    streams[1].ov.hEvent = ev_err;

    DWORD failed = 0;
    int too_much = 0;

    while (p_in != INVALID_HANDLE_VALUE || streams[0].handle != INVALID_HANDLE_VALUE || streams[1].handle != INVALID_HANDLE_VALUE) {
        if (p_in != INVALID_HANDLE_VALUE && !in_pending) {
            if (written == (size_t)input_length) {
                CloseHandle(p_in);
                p_in = INVALID_HANDLE_VALUE;
            } else {
                ResetEvent(ev_in);
                memset(&ov_in, 0, sizeof ov_in);
                ov_in.hEvent = ev_in;
                size_t rem = (size_t)input_length - written;
                DWORD chunk = rem > 65536 ? 65536 : (DWORD)rem;
                DWORD did = 0;
                if (WriteFile(p_in, input + written, chunk, &did, &ov_in)) {
                    written += (size_t)did;
                    continue;
                } else {
                    DWORD err = GetLastError();
                    if (err == ERROR_IO_PENDING) {
                        in_pending = 1;
                    } else {
                        CloseHandle(p_in);
                        p_in = INVALID_HANDLE_VALUE;
                    }
                }
            }
        }

        int restarted = 0;
        for (int i = 0; i < 2; i++) {
            struct tz_os_win_stream *s = &streams[i];
            if (s->handle == INVALID_HANDLE_VALUE || s->pending) continue;
            size_t total_captured = streams[0].length + streams[1].length;
            if ((int64_t)total_captured > TZ_OS_MAX_CAPTURE) {
                too_much = 1;
                break;
            }
            size_t allowed = (size_t)TZ_OS_MAX_CAPTURE + 1 - total_captured;
            DWORD to_read = allowed > sizeof(s->chunk) ? (DWORD)sizeof(s->chunk) : (DWORD)allowed;
            ResetEvent(s->event);
            memset(&s->ov, 0, sizeof s->ov);
            s->ov.hEvent = s->event;
            DWORD got = 0;
            if (ReadFile(s->handle, s->chunk, to_read, &got, &s->ov)) {
                if (got == 0) {
                    CloseHandle(s->handle);
                    s->handle = INVALID_HANDLE_VALUE;
                } else {
                    if (!tz_os_win_stream_append(s, s->chunk, (size_t)got)) {
                        failed = ERROR_NOT_ENOUGH_MEMORY;
                        break;
                    }
                    if ((int64_t)(streams[0].length + streams[1].length) > TZ_OS_MAX_CAPTURE) {
                        too_much = 1;
                        break;
                    }
                }
                restarted = 1;
            } else {
                DWORD err = GetLastError();
                if (err == ERROR_IO_PENDING) {
                    s->pending = 1;
                } else {
                    CloseHandle(s->handle);
                    s->handle = INVALID_HANDLE_VALUE;
                    restarted = 1;
                }
            }
        }
        if (failed || too_much) break;
        if (restarted) continue;

        HANDLE wait_handles[3];
        int wait_kinds[3];
        DWORD wait_count = 0;
        if (p_in != INVALID_HANDLE_VALUE && in_pending) {
            wait_handles[wait_count] = ev_in;
            wait_kinds[wait_count++] = 2;
        }
        for (int i = 0; i < 2; i++) {
            if (streams[i].handle != INVALID_HANDLE_VALUE && streams[i].pending) {
                wait_handles[wait_count] = streams[i].event;
                wait_kinds[wait_count++] = i;
            }
        }
        if (wait_count == 0) break;

        DWORD waited = WaitForMultipleObjects(wait_count, wait_handles, FALSE, INFINITE);
        if (waited < WAIT_OBJECT_0 || waited >= WAIT_OBJECT_0 + wait_count) {
            failed = GetLastError();
            break;
        }
        int kind = wait_kinds[waited - WAIT_OBJECT_0];
        if (kind == 2) {
            DWORD did = 0;
            in_pending = 0;
            if (GetOverlappedResult(p_in, &ov_in, &did, FALSE)) {
                written += (size_t)did;
                if (written == (size_t)input_length) {
                    CloseHandle(p_in);
                    p_in = INVALID_HANDLE_VALUE;
                }
            } else {
                CloseHandle(p_in);
                p_in = INVALID_HANDLE_VALUE;
            }
        } else {
            struct tz_os_win_stream *s = &streams[kind];
            DWORD got = 0;
            s->pending = 0;
            if (GetOverlappedResult(s->handle, &s->ov, &got, FALSE)) {
                if (got == 0) {
                    CloseHandle(s->handle);
                    s->handle = INVALID_HANDLE_VALUE;
                } else {
                    if (!tz_os_win_stream_append(s, s->chunk, (size_t)got)) {
                        failed = ERROR_NOT_ENOUGH_MEMORY;
                        break;
                    }
                    if ((int64_t)(streams[0].length + streams[1].length) > TZ_OS_MAX_CAPTURE) {
                        too_much = 1;
                        break;
                    }
                }
            } else {
                CloseHandle(s->handle);
                s->handle = INVALID_HANDLE_VALUE;
            }
        }
    }

    if (failed || too_much) TerminateProcess(pi.hProcess, 1);
    tz_os_cancel_and_close(&p_in, &ov_in, &in_pending);
    tz_os_cancel_and_close(&streams[0].handle, &streams[0].ov, &streams[0].pending);
    tz_os_cancel_and_close(&streams[1].handle, &streams[1].ov, &streams[1].pending);
    CloseHandle(ev_in);
    CloseHandle(ev_out);
    CloseHandle(ev_err);

    WaitForSingleObject(pi.hProcess, INFINITE);
    DWORD exit_code = 0;
    if (!GetExitCodeProcess(pi.hProcess, &exit_code) && !failed) failed = GetLastError();
    CloseHandle(pi.hThread);
    CloseHandle(pi.hProcess);

    int64_t result = 0;
    if (failed || too_much) {
        result = tz_os_status(TZ_OS_OTHER, too_much ? 0 : (int)failed);
    } else {
        size_t total = 24 + streams[0].length + streams[1].length;
        unsigned char *bytes = (unsigned char *)tsuzuri_alloc((int64_t)total);
        for (int index = 0; index < 4; index++) {
            bytes[index] = (unsigned char)(exit_code >> (8 * index));
            bytes[4 + index] = 0;
        }
        tz_os_put_i64(bytes + 8, (int64_t)streams[0].length);
        tz_os_put_i64(bytes + 16, (int64_t)streams[1].length);
        if (streams[0].length > 0) memcpy(bytes + 24, streams[0].data, streams[0].length);
        if (streams[1].length > 0) memcpy(bytes + 24 + streams[0].length, streams[1].data, streams[1].length);
        output->data = bytes;
        output->length = (int64_t)total;
    }
    free(streams[0].data);
    free(streams[1].data);
    return result;
}

TZ_OS_API int64_t tsuzuri_os_spawn(struct tz_os_buffer *output, const unsigned char *program, int64_t program_length, const unsigned char *arguments, int64_t arguments_length, const unsigned char *input, int64_t input_length) {
    TZ_BLOCKING_ENTER();
    int64_t status = tz_os_spawn_now(output, program, program_length, arguments, arguments_length, input, input_length);
    TZ_BLOCKING_EXIT();
    return status;
}

static int64_t tz_os_random_now(struct tz_os_buffer *output, int64_t count) {
    tz_os_clear(output);
    if (count < 0 || count > TZ_OS_MAX_RANDOM) return tz_os_invalid_input();
    if (count == 0) return 0;
    unsigned char *bytes = (unsigned char *)tsuzuri_alloc(count);
    NTSTATUS st = BCryptGenRandom(NULL, bytes, (ULONG)count, BCRYPT_USE_SYSTEM_PREFERRED_RNG);
    if (!BCRYPT_SUCCESS(st)) {
        tsuzuri_free(bytes);
        return tz_os_status(TZ_OS_OTHER, (int)(uint32_t)st);
    }
    output->data = bytes;
    output->length = count;
    return 0;
}

TZ_OS_API int64_t tsuzuri_os_random(struct tz_os_buffer *output, int64_t count) {
    TZ_BLOCKING_ENTER();
    int64_t status = tz_os_random_now(output, count);
    TZ_BLOCKING_EXIT();
    return status;
}

TZ_OS_API int64_t tsuzuri_os_clock(int32_t operation) {
    if (operation == 0) {
        LARGE_INTEGER frequency, counter;
        if (!QueryPerformanceFrequency(&frequency) || frequency.QuadPart <= 0) return INT64_MIN;
        if (!QueryPerformanceCounter(&counter)) return INT64_MIN;
        int64_t seconds = counter.QuadPart / frequency.QuadPart;
        int64_t rest = counter.QuadPart % frequency.QuadPart;
        if (seconds < 0 || seconds > (INT64_MAX - 1000000000) / INT64_C(1000000000)) return INT64_MIN;
        return seconds * INT64_C(1000000000) + rest * INT64_C(1000000000) / frequency.QuadPart;
    }
    FILETIME ft;
    GetSystemTimePreciseAsFileTime(&ft);
    uint64_t ticks = ((uint64_t)ft.dwHighDateTime << 32) | (uint64_t)ft.dwLowDateTime;
    const uint64_t epoch = UINT64_C(116444736000000000);
    if (ticks < epoch) return INT64_MIN;
    uint64_t diff = ticks - epoch;
    if (diff > (uint64_t)INT64_MAX / 100) return INT64_MIN;
    return (int64_t)(diff * 100);
}

TZ_OS_API int64_t tsuzuri_os_sleep(int64_t milliseconds) {
    if (milliseconds < 0) return tz_os_invalid_input();
    if (milliseconds == 0) return 0;
    int32_t (*hook)(int64_t) = atomic_load_explicit(&tsuzuri_os_sleep_hook, memory_order_acquire);
    int64_t ns = milliseconds > INT64_MAX / 1000000 ? INT64_MAX : milliseconds * 1000000;
    if (hook != NULL && hook(ns)) return 0;
    int64_t start = tsuzuri_os_clock(0);
    if (start == INT64_MIN) {
        Sleep(milliseconds > 0x7fffffff ? 0x7fffffffu : (DWORD)milliseconds);
        return 0;
    }
    int64_t deadline = ns > INT64_MAX - start ? INT64_MAX : start + ns;
    for (;;) {
        int64_t now = tsuzuri_os_clock(0);
        if (now == INT64_MIN || now >= deadline) return 0;
        int64_t rem = deadline - now;
        int64_t wait_ms = (rem + 999999) / 1000000;
        if (wait_ms > 86400000) wait_ms = 86400000;
        Sleep((DWORD)wait_ms);
    }
}

#if defined(TZ_OS_VERSION)
typedef struct tz_os_rtl_version_info {
    ULONG dwOSVersionInfoSize;
    ULONG dwMajorVersion;
    ULONG dwMinorVersion;
    ULONG dwBuildNumber;
    ULONG dwPlatformId;
    WCHAR szCSDVersion[128];
} tz_os_rtl_version_info;

typedef LONG (WINAPI *tz_os_rtl_get_version_fn)(tz_os_rtl_version_info *);

static size_t tz_os_append_ulong(char *dst, size_t pos, ULONG val) {
    char tmp[16];
    size_t k = 0;
    do {
        tmp[k++] = (char)('0' + (val % 10u));
        val /= 10u;
    } while (val > 0);
    while (k > 0) dst[pos++] = tmp[--k];
    return pos;
}

TZ_OS_API int64_t tsuzuri_os_version(struct tz_os_buffer *output) {
    tz_os_clear(output);
    HMODULE ntdll = GetModuleHandleW(L"ntdll.dll");
    if (ntdll == NULL) return tz_os_win_error(GetLastError());
    tz_os_rtl_get_version_fn fn = (tz_os_rtl_get_version_fn)(void *)GetProcAddress(ntdll, "RtlGetVersion");
    if (fn == NULL) return tz_os_win_error(GetLastError());
    tz_os_rtl_version_info info;
    memset(&info, 0, sizeof info);
    info.dwOSVersionInfoSize = (ULONG)sizeof info;
    LONG st = fn(&info);
    if (st != 0) return tz_os_status(TZ_OS_OTHER, (int32_t)st);
    char buf[64];
    size_t n = 0;
    n = tz_os_append_ulong(buf, n, info.dwMajorVersion);
    buf[n++] = '.';
    n = tz_os_append_ulong(buf, n, info.dwMinorVersion);
    buf[n++] = '.';
    n = tz_os_append_ulong(buf, n, info.dwBuildNumber);
    return tz_os_deliver(output, (const unsigned char *)buf, n);
}
#endif
