// The WASI preview1 host of the standard OS API and standard IO (E08 stage C), for `--wasm-host wasi`.
// Freestanding C11 for wasm32: it provides the same tsuzuri_os_* and tsuzuri_io_* functions as os.c and
// io.c, on top of wasi_snapshot_preview1. A function that is not reached brings no import in.
//
// Paths are resolved against the host's preopened directories: the longest preopened name that is a
// prefix of the path (at a "/" boundary) wins, and any other path is relative to the first preopen.
// The `code` of an Os.Error is the WASI errno.
#include <stddef.h>
#include <stdint.h>

#define TZ_WASI_IMPORT(name) __attribute__((import_module("wasi_snapshot_preview1"), import_name(#name)))

struct tz_iovec {
    void *data;
    uint32_t length;
};

TZ_WASI_IMPORT(args_sizes_get) uint32_t tz_wasi_args_sizes_get(uint32_t *count, uint32_t *size);
TZ_WASI_IMPORT(args_get) uint32_t tz_wasi_args_get(char **arguments, char *buffer);
TZ_WASI_IMPORT(environ_sizes_get) uint32_t tz_wasi_environ_sizes_get(uint32_t *count, uint32_t *size);
TZ_WASI_IMPORT(environ_get) uint32_t tz_wasi_environ_get(char **variables, char *buffer);
TZ_WASI_IMPORT(clock_time_get) uint32_t tz_wasi_clock_time_get(uint32_t clock, uint64_t precision, uint64_t *time);
TZ_WASI_IMPORT(fd_close) uint32_t tz_wasi_fd_close(uint32_t descriptor);
TZ_WASI_IMPORT(fd_filestat_get) uint32_t tz_wasi_fd_filestat_get(uint32_t descriptor, void *status);
TZ_WASI_IMPORT(fd_prestat_get) uint32_t tz_wasi_fd_prestat_get(uint32_t descriptor, void *status);
TZ_WASI_IMPORT(fd_prestat_dir_name) uint32_t tz_wasi_fd_prestat_dir_name(uint32_t descriptor, char *path, uint32_t length);
TZ_WASI_IMPORT(fd_read) uint32_t tz_wasi_fd_read(uint32_t descriptor, const struct tz_iovec *vectors, uint32_t count, uint32_t *received);
TZ_WASI_IMPORT(fd_readdir) uint32_t tz_wasi_fd_readdir(uint32_t descriptor, void *buffer, uint32_t length, uint64_t cookie, uint32_t *used);
TZ_WASI_IMPORT(fd_write) uint32_t tz_wasi_fd_write(uint32_t descriptor, const struct tz_iovec *vectors, uint32_t count, uint32_t *written);
TZ_WASI_IMPORT(path_create_directory) uint32_t tz_wasi_path_create_directory(uint32_t descriptor, const char *path, uint32_t length);
TZ_WASI_IMPORT(path_filestat_get) uint32_t tz_wasi_path_filestat_get(uint32_t descriptor, uint32_t lookup, const char *path, uint32_t length, void *status);
TZ_WASI_IMPORT(path_open) uint32_t tz_wasi_path_open(uint32_t descriptor, uint32_t lookup, const char *path, uint32_t length, uint32_t open_flags, uint64_t rights, uint64_t inheriting, uint32_t descriptor_flags, uint32_t *opened);
TZ_WASI_IMPORT(path_remove_directory) uint32_t tz_wasi_path_remove_directory(uint32_t descriptor, const char *path, uint32_t length);
TZ_WASI_IMPORT(path_unlink_file) uint32_t tz_wasi_path_unlink_file(uint32_t descriptor, const char *path, uint32_t length);
TZ_WASI_IMPORT(poll_oneoff) uint32_t tz_wasi_poll_oneoff(const void *subscriptions, void *events, uint32_t count, uint32_t *produced);
TZ_WASI_IMPORT(proc_exit) _Noreturn void tz_wasi_proc_exit(uint32_t code);
TZ_WASI_IMPORT(random_get) uint32_t tz_wasi_random_get(void *buffer, uint32_t length);

extern void *tsuzuri_alloc(int64_t size);
extern void tsuzuri_free(void *pointer);

struct tz_os_buffer {
    unsigned char *data;
    int64_t length;
};

// WASI errno values the mapping needs.
enum {
    TZ_WASI_ACCES = 2,
    TZ_WASI_BADF = 8,
    TZ_WASI_EXIST = 20,
    TZ_WASI_ILSEQ = 25,
    TZ_WASI_INTR = 27,
    TZ_WASI_INVAL = 28,
    TZ_WASI_ISDIR = 31,
    TZ_WASI_NAMETOOLONG = 37,
    TZ_WASI_NOENT = 44,
    TZ_WASI_NOMEM = 48,
    TZ_WASI_NOSYS = 52,
    TZ_WASI_NOTDIR = 54,
    TZ_WASI_NOTEMPTY = 55,
    TZ_WASI_OVERFLOW = 61,
    TZ_WASI_PERM = 63,
    TZ_WASI_ROFS = 69,
    TZ_WASI_NOTCAPABLE = 76,
};

enum {
    TZ_OS_NOT_FOUND = 1,
    TZ_OS_PERMISSION_DENIED = 2,
    TZ_OS_ALREADY_EXISTS = 3,
    TZ_OS_INVALID_INPUT = 4,
    TZ_OS_INVALID_ENCODING = 5,
    TZ_OS_INTERRUPTED = 6,
    TZ_OS_OTHER = 7,
};

// Rights, as wasi-libc requests them.
enum {
    TZ_RIGHT_FD_DATASYNC = 1 << 0,
    TZ_RIGHT_FD_READ = 1 << 1,
    TZ_RIGHT_FD_SEEK = 1 << 2,
    TZ_RIGHT_FD_FDSTAT_SET_FLAGS = 1 << 3,
    TZ_RIGHT_FD_SYNC = 1 << 4,
    TZ_RIGHT_FD_TELL = 1 << 5,
    TZ_RIGHT_FD_WRITE = 1 << 6,
    TZ_RIGHT_FD_ADVISE = 1 << 7,
    TZ_RIGHT_FD_ALLOCATE = 1 << 8,
    TZ_RIGHT_FD_READDIR = 1 << 14,
    TZ_RIGHT_FD_FILESTAT_GET = 1 << 21,
    TZ_RIGHT_FD_FILESTAT_SET_SIZE = 1 << 22,
    TZ_RIGHT_POLL_FD_READWRITE = 1 << 27,
};

#define TZ_OPEN_CREATE 1
#define TZ_OPEN_DIRECTORY 2
#define TZ_OPEN_EXCLUSIVE 4
#define TZ_OPEN_TRUNCATE 8
#define TZ_FD_APPEND 1
#define TZ_LOOKUP_FOLLOW 1
#define TZ_FILETYPE_DIRECTORY 3

#define TZ_RIGHTS_READ (TZ_RIGHT_FD_READ | TZ_RIGHT_FD_SEEK | TZ_RIGHT_FD_TELL | TZ_RIGHT_FD_FILESTAT_GET | TZ_RIGHT_FD_ADVISE | TZ_RIGHT_POLL_FD_READWRITE)
#define TZ_RIGHTS_WRITE (TZ_RIGHT_FD_WRITE | TZ_RIGHT_FD_DATASYNC | TZ_RIGHT_FD_SYNC | TZ_RIGHT_FD_ALLOCATE | TZ_RIGHT_FD_FILESTAT_SET_SIZE | TZ_RIGHT_FD_FILESTAT_GET | TZ_RIGHT_FD_SEEK | TZ_RIGHT_FD_TELL | TZ_RIGHT_POLL_FD_READWRITE)
#define TZ_RIGHTS_DIRECTORY (TZ_RIGHT_FD_READDIR | TZ_RIGHT_FD_FILESTAT_GET)

#define TZ_OS_MAX_RANDOM (INT64_C(1) << 30)

static void *tz_copy(void *destination, const void *source, size_t length) {
    return __builtin_memcpy(destination, source, length);
}

static int tz_compare(const unsigned char *left, size_t left_length, const unsigned char *right, size_t right_length) {
    size_t common = left_length < right_length ? left_length : right_length;
    for (size_t index = 0; index < common; index++) {
        if (left[index] != right[index]) return left[index] < right[index] ? -1 : 1;
    }
    return left_length < right_length ? -1 : left_length > right_length ? 1 : 0;
}

static int64_t tz_status(int kind, int code) {
    return ((int64_t)kind << 32) | (int64_t)(uint32_t)code;
}

static int64_t tz_error(uint32_t error) {
    int kind;
    switch (error) {
    case TZ_WASI_NOENT:
    case TZ_WASI_NOTDIR:
        kind = TZ_OS_NOT_FOUND;
        break;
    case TZ_WASI_ACCES:
    case TZ_WASI_PERM:
    case TZ_WASI_ROFS:
    case TZ_WASI_NOTCAPABLE:
        kind = TZ_OS_PERMISSION_DENIED;
        break;
    case TZ_WASI_EXIST:
        kind = TZ_OS_ALREADY_EXISTS;
        break;
    case TZ_WASI_INVAL:
    case TZ_WASI_NAMETOOLONG:
    case TZ_WASI_ISDIR:
        kind = TZ_OS_INVALID_INPUT;
        break;
    case TZ_WASI_ILSEQ:
        kind = TZ_OS_INVALID_ENCODING;
        break;
    case TZ_WASI_INTR:
        kind = TZ_OS_INTERRUPTED;
        break;
    default:
        kind = TZ_OS_OTHER;
        break;
    }
    return tz_status(kind, (int)error);
}

static int64_t tz_invalid_input(void) {
    return tz_status(TZ_OS_INVALID_INPUT, 0);
}

static void tz_clear(struct tz_os_buffer *output) {
    output->data = NULL;
    output->length = 0;
}

static int tz_has_nul(const unsigned char *bytes, int64_t length) {
    for (int64_t index = 0; index < length; index++) {
        if (bytes[index] == 0) return 1;
    }
    return 0;
}

// Delivers `length` bytes of a tsuzuri_alloc buffer, or frees an empty one.
static int64_t tz_hand_over(struct tz_os_buffer *output, unsigned char *buffer, size_t length) {
    if (length == 0) {
        tsuzuri_free(buffer);
        tz_clear(output);
        return 0;
    }
    output->data = buffer;
    output->length = (int64_t)length;
    return 0;
}

// Preopened directories, loaded once: descriptor 3 and up while the host reports one.
#define TZ_MAX_PREOPENS 16
static struct {
    uint32_t descriptor;
    uint32_t length;
    char *name;
} tz_preopens[TZ_MAX_PREOPENS];
static uint32_t tz_preopen_count;
static int tz_preopens_loaded;
static char tz_preopen_names[4096];
static uint32_t tz_preopen_used;

static void tz_load_preopens(void) {
    if (tz_preopens_loaded) return;
    tz_preopens_loaded = 1;
    for (uint32_t descriptor = 3; tz_preopen_count < TZ_MAX_PREOPENS; descriptor++) {
        struct {
            uint8_t tag;
            uint8_t padding[3];
            uint32_t length;
        } status;
        if (tz_wasi_fd_prestat_get(descriptor, &status) != 0) break;
        if (status.tag != 0) continue;
        if (status.length > sizeof tz_preopen_names - tz_preopen_used) break;
        char *name = tz_preopen_names + tz_preopen_used;
        if (tz_wasi_fd_prestat_dir_name(descriptor, name, status.length) != 0) break;
        tz_preopens[tz_preopen_count].descriptor = descriptor;
        tz_preopens[tz_preopen_count].length = status.length;
        tz_preopens[tz_preopen_count].name = name;
        tz_preopen_count++;
        tz_preopen_used += status.length;
    }
}

// Finds the directory a path starts in and what is left of it. A false result sets `*status`.
static int tz_resolve(const unsigned char *path, int64_t length, uint32_t *directory, const char **rest, uint32_t *rest_length, int64_t *status) {
    if (length < 0 || length > INT32_MAX || tz_has_nul(path, length)) {
        *status = tz_invalid_input();
        return 0;
    }
    tz_load_preopens();
    if (tz_preopen_count == 0) {
        *status = tz_status(TZ_OS_PERMISSION_DENIED, TZ_WASI_NOTCAPABLE);
        return 0;
    }
    int found = -1;
    uint32_t best = 0;
    for (uint32_t index = 0; index < tz_preopen_count; index++) {
        uint32_t size = tz_preopens[index].length;
        if (size > (uint64_t)length) continue;
        if (tz_compare((const unsigned char *)tz_preopens[index].name, size, path, size) != 0) continue;
        int boundary = size == (uint32_t)length || path[size] == '/' || (size > 0 && tz_preopens[index].name[size - 1] == '/');
        // The name "." only matches a relative path as the first preopen does anyway.
        if (boundary && size > best && !(size == 1 && tz_preopens[index].name[0] == '.')) {
            found = (int)index;
            best = size;
        }
    }
    if (found < 0) {
        if (length > 0 && path[0] == '/') {
            *status = tz_status(TZ_OS_PERMISSION_DENIED, TZ_WASI_NOTCAPABLE);
            return 0;
        }
        *directory = tz_preopens[0].descriptor;
        *rest = (const char *)path;
        *rest_length = (uint32_t)length;
    } else {
        const unsigned char *tail = path + best;
        int64_t left = length - best;
        while (left > 0 && *tail == '/') {
            tail++;
            left--;
        }
        *directory = tz_preopens[found].descriptor;
        *rest = left == 0 ? "." : (const char *)tail;
        *rest_length = left == 0 ? 1 : (uint32_t)left;
    }
    return 1;
}

static uint32_t tz_open(uint32_t directory, const char *path, uint32_t length, uint32_t open_flags, uint64_t rights, uint32_t descriptor_flags, uint32_t *opened) {
    for (;;) {
        uint32_t error = tz_wasi_path_open(directory, TZ_LOOKUP_FOLLOW, path, length, open_flags, rights, rights, descriptor_flags, opened);
        if (error != TZ_WASI_INTR) return error;
    }
}

static int tz_is_directory(uint32_t descriptor) {
    unsigned char status[64];
    return tz_wasi_fd_filestat_get(descriptor, status) == 0 && status[16] == TZ_FILETYPE_DIRECTORY;
}

static uint64_t tz_file_size(uint32_t descriptor) {
    unsigned char status[64];
    uint64_t size = 0;
    if (tz_wasi_fd_filestat_get(descriptor, status) != 0) return 0;
    tz_copy(&size, status + 32, sizeof size);
    return size;
}

static int64_t tz_read_file(struct tz_os_buffer *output, uint32_t directory, const char *path, uint32_t path_length) {
    uint32_t descriptor;
    uint32_t error = tz_open(directory, path, path_length, 0, TZ_RIGHTS_READ, 0, &descriptor);
    if (error != 0) return tz_error(error);
    if (tz_is_directory(descriptor)) {
        tz_wasi_fd_close(descriptor);
        return tz_error(TZ_WASI_ISDIR);
    }
    uint64_t size = tz_file_size(descriptor);
    size_t capacity = size > 0 && size < (UINT64_C(1) << 30) ? (size_t)size + 1 : 4096;
    size_t length = 0;
    unsigned char *buffer = tsuzuri_alloc((int64_t)capacity);
    for (;;) {
        if (length == capacity) {
            if (capacity > (size_t)INT32_MAX / 2) {
                tsuzuri_free(buffer);
                tz_wasi_fd_close(descriptor);
                return tz_status(TZ_OS_OTHER, TZ_WASI_NOMEM);
            }
            unsigned char *larger = tsuzuri_alloc((int64_t)capacity * 2);
            tz_copy(larger, buffer, length);
            tsuzuri_free(buffer);
            buffer = larger;
            capacity *= 2;
        }
        struct tz_iovec vector = { buffer + length, (uint32_t)(capacity - length) };
        uint32_t received = 0;
        error = tz_wasi_fd_read(descriptor, &vector, 1, &received);
        if (error == TZ_WASI_INTR) continue;
        if (error != 0) {
            tsuzuri_free(buffer);
            tz_wasi_fd_close(descriptor);
            return tz_error(error);
        }
        if (received == 0) break;
        length += received;
    }
    tz_wasi_fd_close(descriptor);
    return tz_hand_over(output, buffer, length);
}

static void tz_put_i64(unsigned char *at, int64_t value) {
    for (int index = 0; index < 8; index++) at[index] = (unsigned char)((uint64_t)value >> (8 * index));
}

// The 24 bytes of File.metadata, as in os.c: the kind (0 file, 1 directory, 2 symbolic link, 3 other),
// then the size and the modification time in Unix nanoseconds. `follow` selects a symbolic link's target.
static int64_t tz_stat(struct tz_os_buffer *output, uint32_t directory, const char *path, uint32_t path_length, int follow) {
    unsigned char status[64];
    uint32_t error = tz_wasi_path_filestat_get(directory, follow ? TZ_LOOKUP_FOLLOW : 0, path, path_length, status);
    if (error != 0) return tz_error(error);
    uint64_t size, modified;
    tz_copy(&size, status + 32, sizeof size);
    tz_copy(&modified, status + 48, sizeof modified);
    unsigned char record[24];
    for (size_t index = 0; index < sizeof record; index++) record[index] = 0;
    record[0] = status[16] == 4 ? 0 : status[16] == TZ_FILETYPE_DIRECTORY ? 1 : status[16] == 7 ? 2 : 3;
    tz_put_i64(record + 8, size > (uint64_t)INT64_MAX ? INT64_MAX : (int64_t)size);
    tz_put_i64(record + 16, modified > (uint64_t)INT64_MAX ? INT64_MAX : (int64_t)modified);
    unsigned char *copy = tsuzuri_alloc((int64_t)sizeof record);
    tz_copy(copy, record, sizeof record);
    return tz_hand_over(output, copy, sizeof record);
}

// Heap sort of offsets into a buffer of NUL-terminated names, by the names' bytes.
static size_t tz_name_length(const unsigned char *names, uint32_t offset) {
    size_t length = 0;
    while (names[offset + length] != 0) length++;
    return length;
}

static int tz_name_before(const unsigned char *names, uint32_t left, uint32_t right) {
    return tz_compare(names + left, tz_name_length(names, left), names + right, tz_name_length(names, right)) < 0;
}

static void tz_sift(const unsigned char *names, uint32_t *offsets, size_t root, size_t end) {
    for (;;) {
        size_t child = 2 * root + 1;
        if (child >= end) return;
        if (child + 1 < end && tz_name_before(names, offsets[child], offsets[child + 1])) child++;
        if (!tz_name_before(names, offsets[root], offsets[child])) return;
        uint32_t swap = offsets[root];
        offsets[root] = offsets[child];
        offsets[child] = swap;
        root = child;
    }
}

static void tz_sort_names(const unsigned char *names, uint32_t *offsets, size_t count) {
    if (count < 2) return;
    for (size_t index = count / 2; index > 0; index--) tz_sift(names, offsets, index - 1, count);
    for (size_t end = count - 1; end > 0; end--) {
        uint32_t swap = offsets[0];
        offsets[0] = offsets[end];
        offsets[end] = swap;
        tz_sift(names, offsets, 0, end);
    }
}

// Lists the entries of a directory except "." and "..", sorted by their bytes, each followed by a NUL.
static int64_t tz_list_directory(struct tz_os_buffer *output, uint32_t directory, const char *path, uint32_t path_length) {
    uint32_t descriptor;
    uint32_t error = tz_open(directory, path, path_length, TZ_OPEN_DIRECTORY, TZ_RIGHTS_DIRECTORY, 0, &descriptor);
    if (error != 0) return tz_error(error);
    size_t buffer_size = 16384;
    unsigned char *entries = tsuzuri_alloc((int64_t)buffer_size);
    size_t names_capacity = 1024, names_length = 0;
    unsigned char *names = tsuzuri_alloc((int64_t)names_capacity);
    size_t offsets_capacity = 64, count = 0;
    uint32_t *offsets = tsuzuri_alloc((int64_t)(offsets_capacity * sizeof(uint32_t)));
    uint64_t cookie = 0;
    int64_t status = 0;
    for (;;) {
        uint32_t used = 0;
        error = tz_wasi_fd_readdir(descriptor, entries, (uint32_t)buffer_size, cookie, &used);
        if (error != 0) {
            status = tz_error(error);
            break;
        }
        size_t offset = 0;
        int parsed = 0;
        while (used - offset >= 24) {
            uint64_t next;
            uint32_t length;
            tz_copy(&next, entries + offset, sizeof next);
            tz_copy(&length, entries + offset + 16, sizeof length);
            if (used - offset - 24 < length) break;
            const unsigned char *name = entries + offset + 24;
            offset += 24 + length;
            cookie = next;
            parsed = 1;
            if ((length == 1 && name[0] == '.') || (length == 2 && name[0] == '.' && name[1] == '.')) continue;
            if (names_length + length + 1 > names_capacity) {
                size_t larger_capacity = names_capacity;
                while (names_length + length + 1 > larger_capacity) larger_capacity *= 2;
                unsigned char *larger = tsuzuri_alloc((int64_t)larger_capacity);
                tz_copy(larger, names, names_length);
                tsuzuri_free(names);
                names = larger;
                names_capacity = larger_capacity;
            }
            if (count == offsets_capacity) {
                uint32_t *larger = tsuzuri_alloc((int64_t)(offsets_capacity * 2 * sizeof(uint32_t)));
                tz_copy(larger, offsets, count * sizeof(uint32_t));
                tsuzuri_free(offsets);
                offsets = larger;
                offsets_capacity *= 2;
            }
            offsets[count++] = (uint32_t)names_length;
            tz_copy(names + names_length, name, length);
            names[names_length + length] = 0;
            names_length += length + 1;
        }
        if (used < buffer_size) break;
        if (!parsed) {
            // One entry is larger than the buffer.
            tsuzuri_free(entries);
            buffer_size *= 2;
            entries = tsuzuri_alloc((int64_t)buffer_size);
        }
    }
    tz_wasi_fd_close(descriptor);
    tsuzuri_free(entries);
    if (status == 0) {
        tz_sort_names(names, offsets, count);
        unsigned char *joined = tsuzuri_alloc((int64_t)(names_length == 0 ? 1 : names_length));
        size_t at = 0;
        for (size_t index = 0; index < count; index++) {
            size_t length = tz_name_length(names, offsets[index]) + 1;
            tz_copy(joined + at, names + offsets[index], length);
            at += length;
        }
        status = tz_hand_over(output, joined, names_length);
    }
    tsuzuri_free(names);
    tsuzuri_free(offsets);
    return status;
}

static int tz_variable_matches(const char *entry, const unsigned char *name, int64_t length) {
    for (int64_t index = 0; index < length; index++) {
        if ((unsigned char)entry[index] != name[index]) return 0;
    }
    return entry[length] == '=';
}

// A variable's value, or kind 1 with code 0 when it is not set.
static int64_t tz_environment(struct tz_os_buffer *output, const unsigned char *name, int64_t length) {
    if (length == 0) return tz_invalid_input();
    for (int64_t index = 0; index < length; index++) {
        if (name[index] == '=') return tz_invalid_input();
    }
    uint32_t count = 0, size = 0;
    uint32_t error = tz_wasi_environ_sizes_get(&count, &size);
    if (error != 0) return tz_error(error);
    if (count == 0) return tz_status(TZ_OS_NOT_FOUND, 0);
    char **variables = tsuzuri_alloc((int64_t)count * (int64_t)sizeof(char *));
    char *buffer = tsuzuri_alloc((int64_t)size + 1);
    int64_t status = tz_status(TZ_OS_NOT_FOUND, 0);
    error = tz_wasi_environ_get(variables, buffer);
    if (error != 0) {
        status = tz_error(error);
    } else {
        for (uint32_t index = 0; index < count; index++) {
            if (!tz_variable_matches(variables[index], name, length)) continue;
            const char *value = variables[index] + length + 1;
            size_t value_length = 0;
            while (value[value_length] != 0) value_length++;
            unsigned char *copy = value_length == 0 ? NULL : tsuzuri_alloc((int64_t)value_length);
            if (value_length != 0) tz_copy(copy, value, value_length);
            tz_clear(output);
            output->data = copy;
            output->length = (int64_t)value_length;
            status = 0;
            break;
        }
    }
    tsuzuri_free(variables);
    tsuzuri_free(buffer);
    return status;
}

// op 0 reads a file, 1 lists a directory, 2 returns the first preopened directory's name, 3 reads an
// environment variable (a missing one is kind 1 with code 0), and 4 and 5 describe a path, following a
// symbolic link (4) or not (5).
int64_t tsuzuri_os_read(struct tz_os_buffer *output, int32_t operation, const unsigned char *path, int64_t length) {
    tz_clear(output);
    if (operation == 2) {
        tz_load_preopens();
        if (tz_preopen_count == 0) return tz_status(TZ_OS_NOT_FOUND, 0);
        unsigned char *name = tsuzuri_alloc((int64_t)tz_preopens[0].length + 1);
        tz_copy(name, tz_preopens[0].name, tz_preopens[0].length);
        return tz_hand_over(output, name, tz_preopens[0].length);
    }
    if (operation == 3) return tz_environment(output, path, length);
    uint32_t directory = 0, rest_length = 0;
    const char *rest = NULL;
    int64_t status = 0;
    if (!tz_resolve(path, length, &directory, &rest, &rest_length, &status)) return status;
    if (operation == 0) return tz_read_file(output, directory, rest, rest_length);
    if (operation == 1) return tz_list_directory(output, directory, rest, rest_length);
    if (operation == 4 || operation == 5) return tz_stat(output, directory, rest, rest_length, operation == 4);
    return tz_invalid_input();
}

void tsuzuri_os_set_args(int32_t count, char **arguments) {
    (void)count;
    (void)arguments;
}

// The arguments after the program name, each followed by a NUL.
int64_t tsuzuri_os_args(struct tz_os_buffer *output) {
    tz_clear(output);
    uint32_t count = 0, size = 0;
    uint32_t error = tz_wasi_args_sizes_get(&count, &size);
    if (error != 0) return tz_error(error);
    if (count <= 1) return 0;
    char **arguments = tsuzuri_alloc((int64_t)count * (int64_t)sizeof(char *));
    char *buffer = tsuzuri_alloc((int64_t)size + 1);
    int64_t status = 0;
    error = tz_wasi_args_get(arguments, buffer);
    if (error != 0) {
        status = tz_error(error);
    } else {
        size_t total = 0;
        for (uint32_t index = 1; index < count; index++) {
            size_t length = 0;
            while (arguments[index][length] != 0) length++;
            total += length + 1;
        }
        unsigned char *joined = tsuzuri_alloc((int64_t)total);
        size_t at = 0;
        for (uint32_t index = 1; index < count; index++) {
            size_t length = 0;
            while (arguments[index][length] != 0) length++;
            tz_copy(joined + at, arguments[index], length + 1);
            at += length + 1;
        }
        status = tz_hand_over(output, joined, total);
    }
    tsuzuri_free(arguments);
    tsuzuri_free(buffer);
    return status;
}

static int64_t tz_write_all(uint32_t descriptor, const unsigned char *data, int64_t length) {
    int64_t offset = 0;
    while (offset < length) {
        struct tz_iovec vector = { (void *)(data + offset), (uint32_t)(length - offset > INT32_MAX ? INT32_MAX : length - offset) };
        uint32_t written = 0;
        uint32_t error = tz_wasi_fd_write(descriptor, &vector, 1, &written);
        if (error == TZ_WASI_INTR) continue;
        if (error != 0) return tz_error(error);
        offset += written;
    }
    return 0;
}

static int64_t tz_write_file(uint32_t directory, const char *path, uint32_t path_length, uint32_t open_flags, uint32_t descriptor_flags, const unsigned char *data, int64_t data_length) {
    uint32_t descriptor;
    uint32_t error = tz_open(directory, path, path_length, open_flags, TZ_RIGHTS_WRITE, descriptor_flags, &descriptor);
    if (error != 0) return tz_error(error);
    int64_t status = tz_write_all(descriptor, data, data_length);
    error = tz_wasi_fd_close(descriptor);
    return status != 0 ? status : error != 0 && error != TZ_WASI_INTR ? tz_error(error) : 0;
}

// op 0 creates or truncates and writes, 1 creates or appends, 2 makes a directory (parents are not
// created), 3 removes an empty directory, and 4 unlinks a file or a symbolic link.
int64_t tsuzuri_os_write(int32_t operation, const unsigned char *path, int64_t path_length, const unsigned char *data, int64_t data_length) {
    uint32_t directory = 0, rest_length = 0;
    const char *rest = NULL;
    int64_t status = 0;
    if (!tz_resolve(path, path_length, &directory, &rest, &rest_length, &status)) return status;
    uint32_t error;
    switch (operation) {
    case 0:
        return tz_write_file(directory, rest, rest_length, TZ_OPEN_CREATE | TZ_OPEN_TRUNCATE, 0, data, data_length);
    case 1:
        return tz_write_file(directory, rest, rest_length, TZ_OPEN_CREATE, TZ_FD_APPEND, data, data_length);
    case 2:
        error = tz_wasi_path_create_directory(directory, rest, rest_length);
        break;
    case 3:
        error = tz_wasi_path_remove_directory(directory, rest, rest_length);
        break;
    case 4:
        error = tz_wasi_path_unlink_file(directory, rest, rest_length);
        break;
    default:
        return tz_invalid_input();
    }
    return error == 0 ? 0 : tz_error(error);
}

// WASI has no way to start a process.
int64_t tsuzuri_os_spawn(struct tz_os_buffer *output, const unsigned char *program, int64_t program_length, const unsigned char *arguments, int64_t arguments_length, const unsigned char *input, int64_t input_length) {
    (void)program;
    (void)program_length;
    (void)arguments;
    (void)arguments_length;
    (void)input;
    (void)input_length;
    tz_clear(output);
    return tz_status(TZ_OS_OTHER, TZ_WASI_NOSYS);
}

int64_t tsuzuri_os_random(struct tz_os_buffer *output, int64_t count) {
    tz_clear(output);
    if (count < 0 || count > TZ_OS_MAX_RANDOM) return tz_invalid_input();
    if (count == 0) return 0;
    unsigned char *bytes = tsuzuri_alloc(count);
    for (int64_t offset = 0; offset < count;) {
        uint32_t chunk = (uint32_t)(count - offset < 256 ? count - offset : 256);
        uint32_t error = tz_wasi_random_get(bytes + offset, chunk);
        if (error != 0) {
            tsuzuri_free(bytes);
            return tz_error(error);
        }
        offset += chunk;
    }
    output->data = bytes;
    output->length = count;
    return 0;
}

// op 0 is the monotonic clock and 1 the Unix clock, both in nanoseconds; INT64_MIN reports failure.
int64_t tsuzuri_os_clock(int32_t operation) {
    uint64_t now = 0;
    if (tz_wasi_clock_time_get(operation == 0 ? 1 : 0, 1, &now) != 0) return INT64_MIN;
    return now > (uint64_t)INT64_MAX ? INT64_MIN : (int64_t)now;
}

int64_t tsuzuri_os_sleep(int64_t milliseconds) {
    if (milliseconds < 0) return tz_invalid_input();
    if (milliseconds == 0) return 0;
    // A subscription is 48 bytes: userdata, tag 0 (clock), then the clock id, timeout, precision, and flags.
    unsigned char subscription[48];
    unsigned char event[32];
    for (size_t index = 0; index < sizeof subscription; index++) subscription[index] = 0;
    uint32_t clock = 1;
    uint64_t timeout = (uint64_t)milliseconds * UINT64_C(1000000);
    if ((uint64_t)milliseconds > UINT64_MAX / UINT64_C(1000000)) timeout = UINT64_MAX;
    tz_copy(subscription + 16, &clock, sizeof clock);
    tz_copy(subscription + 24, &timeout, sizeof timeout);
    uint32_t produced = 0;
    uint32_t error = tz_wasi_poll_oneoff(subscription, event, 1, &produced);
    return error == 0 ? 0 : tz_error(error);
}

// Open files (File.Handle), as in os.c: a handle is (generation << 32) | (slot + 1).
struct tz_os_file {
    uint32_t descriptor;
    int64_t generation;
    int readable;
    int writable;
};

static struct tz_os_file *tz_os_files;
static size_t tz_os_file_capacity;
static size_t tz_os_files_open;
static int64_t tz_os_generation;

static struct tz_os_file *tz_os_file_of(int64_t handle) {
    if (handle <= 0) return NULL;
    uint64_t slot = (uint64_t)handle & UINT64_C(0xffffffff);
    int64_t generation = handle >> 32;
    if (slot == 0 || slot > tz_os_file_capacity) return NULL;
    struct tz_os_file *file = &tz_os_files[slot - 1];
    return file->generation == generation && generation != 0 ? file : NULL;
}

static int64_t tz_bad_handle(void) {
    return tz_status(TZ_OS_INVALID_INPUT, TZ_WASI_BADF);
}

int64_t tsuzuri_os_open(int32_t mode, const unsigned char *path, int64_t length) {
    uint32_t directory = 0, rest_length = 0;
    const char *rest = NULL;
    int64_t status = 0;
    if (!tz_resolve(path, length, &directory, &rest, &rest_length, &status)) return -status;
    uint32_t open_flags = 0, descriptor_flags = 0;
    uint64_t rights = TZ_RIGHTS_WRITE;
    int readable = 0, writable = 1;
    switch (mode) {
    case 0:
        rights = TZ_RIGHTS_READ;
        readable = 1;
        writable = 0;
        break;
    case 1:
        open_flags = TZ_OPEN_CREATE | TZ_OPEN_TRUNCATE;
        break;
    case 2:
        open_flags = TZ_OPEN_CREATE;
        descriptor_flags = TZ_FD_APPEND;
        break;
    case 3:
        open_flags = TZ_OPEN_CREATE | TZ_OPEN_EXCLUSIVE;
        break;
    default:
        return -tz_invalid_input();
    }
    uint32_t descriptor;
    uint32_t error = tz_open(directory, rest, rest_length, open_flags, rights, descriptor_flags, &descriptor);
    if (error != 0) return -tz_error(error);
    if (tz_is_directory(descriptor)) {
        tz_wasi_fd_close(descriptor);
        return -tz_error(TZ_WASI_ISDIR);
    }
    if (tz_os_generation >= INT32_MAX) {
        tz_wasi_fd_close(descriptor);
        return -tz_status(TZ_OS_OTHER, TZ_WASI_OVERFLOW);
    }
    size_t slot = 0;
    while (slot < tz_os_file_capacity && tz_os_files[slot].generation != 0) slot++;
    if (slot == tz_os_file_capacity) {
        size_t next = tz_os_file_capacity == 0 ? 8 : tz_os_file_capacity * 2;
        struct tz_os_file *larger = tsuzuri_alloc((int64_t)(next * sizeof(struct tz_os_file)));
        if (tz_os_file_capacity != 0) tz_copy(larger, tz_os_files, tz_os_file_capacity * sizeof(struct tz_os_file));
        for (size_t index = tz_os_file_capacity; index < next; index++) larger[index].generation = 0;
        tsuzuri_free(tz_os_files);
        tz_os_files = larger;
        tz_os_file_capacity = next;
    }
    struct tz_os_file *file = &tz_os_files[slot];
    file->descriptor = descriptor;
    file->generation = ++tz_os_generation;
    file->readable = readable;
    file->writable = writable;
    tz_os_files_open++;
    return (file->generation << 32) | (int64_t)(slot + 1);
}

int64_t tsuzuri_os_handle(struct tz_os_buffer *output, int32_t operation, int64_t handle, int64_t count, const unsigned char *data, int64_t data_length) {
    tz_clear(output);
    struct tz_os_file *file = tz_os_file_of(handle);
    if (file == NULL) return tz_bad_handle();
    switch (operation) {
    case 0: {
        if (!file->readable) return tz_bad_handle();
        if (count < 0 || count > TZ_OS_MAX_RANDOM) return tz_invalid_input();
        if (count == 0) return 0;
        unsigned char *bytes = tsuzuri_alloc(count);
        for (;;) {
            struct tz_iovec vector = { bytes, (uint32_t)count };
            uint32_t received = 0;
            uint32_t error = tz_wasi_fd_read(file->descriptor, &vector, 1, &received);
            if (error == TZ_WASI_INTR) continue;
            if (error != 0) {
                tsuzuri_free(bytes);
                return tz_error(error);
            }
            return tz_hand_over(output, bytes, received);
        }
    }
    case 1:
        if (!file->writable) return tz_bad_handle();
        return tz_write_all(file->descriptor, data, data_length);
    case 2:
        return 0;
    default:
        return tz_invalid_input();
    }
}

int64_t tsuzuri_os_close(int64_t handle) {
    struct tz_os_file *file = tz_os_file_of(handle);
    if (file == NULL) return tz_bad_handle();
    uint32_t error = tz_wasi_fd_close(file->descriptor);
    file->generation = 0;
    if (--tz_os_files_open == 0) {
        tsuzuri_free(tz_os_files);
        tz_os_files = NULL;
        tz_os_file_capacity = 0;
    }
    return error != 0 && error != TZ_WASI_INTR ? tz_error(error) : 0;
}

// Standard IO, with the contract of io.c: read_line returns 0 for a line, 1 at the end of the input,
// and 2 for a failure; write returns 0 or 1.
static unsigned char tz_input[4096];
static uint32_t tz_input_position, tz_input_length;

// The next input byte, or a negative number at the end (-1) or on a failure (-2).
static int tz_next_byte(void) {
    if (tz_input_position == tz_input_length) {
        for (;;) {
            struct tz_iovec vector = { tz_input, sizeof tz_input };
            uint32_t received = 0;
            uint32_t error = tz_wasi_fd_read(0, &vector, 1, &received);
            if (error == TZ_WASI_INTR) continue;
            if (error != 0) return -2;
            if (received == 0) return -1;
            tz_input_position = 0;
            tz_input_length = received;
            break;
        }
    }
    return tz_input[tz_input_position++];
}

int32_t tsuzuri_io_read_line(struct tz_os_buffer *output) {
    tz_clear(output);
    int64_t capacity = 0;
    for (;;) {
        int value = tz_next_byte();
        if (value < 0) {
            if (value == -2) {
                tsuzuri_free(output->data);
                tz_clear(output);
                return 2;
            }
            return output->length == 0 ? 1 : 0;
        }
        if (value == '\n') {
            if (output->length > 0 && output->data[output->length - 1] == '\r') output->length--;
            return 0;
        }
        if (output->length == capacity) {
            int64_t next_capacity = capacity == 0 ? 256 : capacity * 2;
            if (capacity > INT32_MAX / 2) {
                tsuzuri_free(output->data);
                tz_clear(output);
                return 2;
            }
            unsigned char *next = tsuzuri_alloc(next_capacity);
            if (output->length > 0) tz_copy(next, output->data, (size_t)output->length);
            tsuzuri_free(output->data);
            output->data = next;
            capacity = next_capacity;
        }
        output->data[output->length++] = (unsigned char)value;
    }
}

int32_t tsuzuri_io_write(int32_t destination, const unsigned char *data, int64_t length) {
    if ((destination != 1 && destination != 2) || length < 0) return 1;
    return tz_write_all((uint32_t)destination, data, length) == 0 ? 0 : 1;
}

#ifdef TZ_WASI_START
extern int32_t tsuzuri_main(void);

// The WASI command entry: runs the program, and exits with its `IO<i32>` value when it has one.
__attribute__((export_name("_start"))) void _start(void) {
    int32_t code = tsuzuri_main();
#ifdef TZ_WASI_EXIT_CODE
    if (code != 0) tz_wasi_proc_exit((uint32_t)code);
#else
    (void)code;
#endif
}
#endif
