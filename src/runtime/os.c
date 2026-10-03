// Operating-system primitives of the standard File, Dir, Env, Time, and Random modules (E08).
// POSIX only (macOS and Linux). The driver links this file when the program reaches Os.__*.
//
// Every function that returns bytes writes its descriptor first and on every path (NULL and 0
// on failure), and returns a status: 0 on success or (kind << 32) | (code & 0xffffffff) with
// kind 1 NotFound, 2 PermissionDenied, 3 AlreadyExists, 4 InvalidInput, 5 InvalidEncoding,
// 6 Interrupted, 7 Other. The feature macros must come before the first include.
#if defined(__APPLE__) && !defined(_DARWIN_C_SOURCE)
#define _DARWIN_C_SOURCE
#endif
#if defined(__linux__) && !defined(_GNU_SOURCE)
#define _GNU_SOURCE
#endif

#include <dirent.h>
#include <errno.h>
#include <fcntl.h>
#include <poll.h>
#include <signal.h>
#include <spawn.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/wait.h>
#include <time.h>
#include <unistd.h>
#if defined(__APPLE__)
#include <crt_externs.h>
#include <sys/random.h>
#define TZ_OS_ENVIRON (*_NSGetEnviron())
#else
extern char **environ;
#define TZ_OS_ENVIRON environ
#endif

#define TZ_OS_API __attribute__((weak, visibility("hidden")))

extern void *tsuzuri_alloc(int64_t size);
extern void tsuzuri_free(void *pointer);

struct tz_os_buffer {
    unsigned char *data;
    int64_t length;
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

#define TZ_OS_MAX_RANDOM (INT64_C(1) << 30)
#define TZ_OS_MAX_CURRENT_DIR (1024 * 1024)

static int64_t tz_os_status(int kind, int code) {
    return ((int64_t)kind << 32) | (int64_t)(uint32_t)code;
}

static int64_t tz_os_error(int error) {
    int kind;
    switch (error) {
    case ENOENT:
    case ENOTDIR:
        kind = TZ_OS_NOT_FOUND;
        break;
    case EACCES:
    case EPERM:
    case EROFS:
        kind = TZ_OS_PERMISSION_DENIED;
        break;
    case EEXIST:
        kind = TZ_OS_ALREADY_EXISTS;
        break;
    case EINVAL:
    case ENAMETOOLONG:
    case EISDIR:
        kind = TZ_OS_INVALID_INPUT;
        break;
    case EILSEQ:
        kind = TZ_OS_INVALID_ENCODING;
        break;
    case EINTR:
        kind = TZ_OS_INTERRUPTED;
        break;
    default:
        kind = TZ_OS_OTHER;
        break;
    }
    return tz_os_status(kind, error);
}

static int64_t tz_os_invalid_input(void) {
    return tz_os_status(TZ_OS_INVALID_INPUT, 0);
}

static void tz_os_clear(struct tz_os_buffer *output) {
    output->data = NULL;
    output->length = 0;
}

// Copies `length` bytes into a buffer the caller of the descriptor owns.
static int64_t tz_os_deliver(struct tz_os_buffer *output, const unsigned char *bytes, size_t length) {
    tz_os_clear(output);
    if (length == 0) return 0;
    if (length > (size_t)INT64_MAX) return tz_os_status(TZ_OS_OTHER, ENOMEM);
    unsigned char *copy = tsuzuri_alloc((int64_t)length);
    memcpy(copy, bytes, length);
    output->data = copy;
    output->length = (int64_t)length;
    return 0;
}

// A NUL-terminated copy of a path or name; NULL with *status set when it holds a NUL byte.
static char *tz_os_name(const unsigned char *bytes, int64_t length, int64_t *status) {
    if (length < 0 || (uint64_t)length >= (uint64_t)SIZE_MAX || (length > 0 && memchr(bytes, 0, (size_t)length) != NULL)) {
        *status = tz_os_invalid_input();
        return NULL;
    }
    char *name = malloc((size_t)length + 1);
    if (name == NULL) {
        *status = tz_os_status(TZ_OS_OTHER, ENOMEM);
        return NULL;
    }
    if (length > 0) memcpy(name, bytes, (size_t)length);
    name[length] = '\0';
    return name;
}

static int tz_os_open(const char *name, int flags) {
    for (;;) {
        int descriptor = open(name, flags | O_CLOEXEC, 0666);
        if (descriptor >= 0 || errno != EINTR) return descriptor;
    }
}

static int64_t tz_os_read_file(struct tz_os_buffer *output, const char *name) {
    int descriptor = tz_os_open(name, O_RDONLY);
    if (descriptor < 0) return tz_os_error(errno);
    struct stat information;
    if (fstat(descriptor, &information) != 0) {
        int error = errno;
        close(descriptor);
        return tz_os_error(error);
    }
    if (S_ISDIR(information.st_mode)) {
        close(descriptor);
        return tz_os_error(EISDIR);
    }
    size_t capacity = information.st_size > 0 && (uint64_t)information.st_size < (uint64_t)SIZE_MAX / 2
        ? (size_t)information.st_size + 1
        : 4096;
    size_t length = 0;
    unsigned char *buffer = malloc(capacity);
    if (buffer == NULL) {
        close(descriptor);
        return tz_os_status(TZ_OS_OTHER, ENOMEM);
    }
    for (;;) {
        if (length == capacity) {
            if (capacity > (size_t)INT64_MAX / 2) {
                free(buffer);
                close(descriptor);
                return tz_os_status(TZ_OS_OTHER, EFBIG);
            }
            unsigned char *larger = realloc(buffer, capacity * 2);
            if (larger == NULL) {
                free(buffer);
                close(descriptor);
                return tz_os_status(TZ_OS_OTHER, ENOMEM);
            }
            buffer = larger;
            capacity *= 2;
        }
        ssize_t count = read(descriptor, buffer + length, capacity - length);
        if (count < 0) {
            if (errno == EINTR) continue;
            int error = errno;
            free(buffer);
            close(descriptor);
            return tz_os_error(error);
        }
        if (count == 0) break;
        length += (size_t)count;
    }
    close(descriptor);
    int64_t status = tz_os_deliver(output, buffer, length);
    free(buffer);
    return status;
}

static int tz_os_compare_names(const void *left, const void *right) {
    const char *first = *(const char *const *)left;
    const char *second = *(const char *const *)right;
    size_t first_length = strlen(first);
    size_t second_length = strlen(second);
    int order = memcmp(first, second, first_length < second_length ? first_length : second_length);
    if (order != 0) return order;
    return first_length < second_length ? -1 : first_length > second_length ? 1 : 0;
}

// Lists the entries of a directory except "." and "..", sorted by their bytes, each followed by a NUL.
static int64_t tz_os_list_directory(struct tz_os_buffer *output, const char *name) {
    DIR *directory = opendir(name);
    if (directory == NULL) return tz_os_error(errno);
    char **names = NULL;
    size_t count = 0, capacity = 0, total = 0;
    int64_t status = 0;
    for (;;) {
        errno = 0;
        struct dirent *entry = readdir(directory);
        if (entry == NULL) {
            if (errno != 0) status = tz_os_error(errno);
            break;
        }
        if (strcmp(entry->d_name, ".") == 0 || strcmp(entry->d_name, "..") == 0) continue;
        if (count == capacity) {
            size_t next = capacity == 0 ? 16 : capacity * 2;
            char **larger = realloc(names, next * sizeof(char *));
            if (larger == NULL) {
                status = tz_os_status(TZ_OS_OTHER, ENOMEM);
                break;
            }
            names = larger;
            capacity = next;
        }
        char *copy = strdup(entry->d_name);
        if (copy == NULL) {
            status = tz_os_status(TZ_OS_OTHER, ENOMEM);
            break;
        }
        names[count++] = copy;
        total += strlen(copy) + 1;
    }
    closedir(directory);
    if (status == 0) {
        qsort(names, count, sizeof(char *), tz_os_compare_names);
        unsigned char *joined = total == 0 ? NULL : malloc(total);
        if (total != 0 && joined == NULL) {
            status = tz_os_status(TZ_OS_OTHER, ENOMEM);
        } else {
            size_t offset = 0;
            for (size_t index = 0; index < count; index++) {
                size_t length = strlen(names[index]) + 1;
                memcpy(joined + offset, names[index], length);
                offset += length;
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
    size_t capacity = 4096;
    for (;;) {
        char *buffer = malloc(capacity);
        if (buffer == NULL) return tz_os_status(TZ_OS_OTHER, ENOMEM);
        if (getcwd(buffer, capacity) != NULL) {
            int64_t status = tz_os_deliver(output, (const unsigned char *)buffer, strlen(buffer));
            free(buffer);
            return status;
        }
        int error = errno;
        free(buffer);
        if (error != ERANGE) return tz_os_error(error);
        if (capacity >= TZ_OS_MAX_CURRENT_DIR) return tz_os_status(TZ_OS_OTHER, ERANGE);
        capacity *= 2;
    }
}

static void tz_os_put_i64(unsigned char *at, int64_t value) {
    for (int index = 0; index < 8; index++) at[index] = (unsigned char)((uint64_t)value >> (8 * index));
}

// The 24 bytes of File.metadata: the kind (0 file, 1 directory, 2 symbolic link, 3 other), then the size
// and the modification time in Unix nanoseconds, both little endian. `follow` selects stat over lstat.
static int64_t tz_os_stat(struct tz_os_buffer *output, const char *name, int follow) {
    struct stat information;
    if ((follow ? stat(name, &information) : lstat(name, &information)) != 0) return tz_os_error(errno);
    unsigned char record[24];
    memset(record, 0, sizeof record);
    record[0] = S_ISREG(information.st_mode) ? 0 : S_ISDIR(information.st_mode) ? 1 : S_ISLNK(information.st_mode) ? 2 : 3;
    tz_os_put_i64(record + 8, (int64_t)information.st_size);
#if defined(__APPLE__)
    struct timespec modified = information.st_mtimespec;
#else
    struct timespec modified = information.st_mtim;
#endif
    int64_t nanoseconds;
    if (__builtin_mul_overflow((int64_t)modified.tv_sec, INT64_C(1000000000), &nanoseconds) || __builtin_add_overflow(nanoseconds, (int64_t)modified.tv_nsec, &nanoseconds)) {
        // Beyond the year 2262 or before 1677 the time is clamped to what an i64 holds.
        nanoseconds = modified.tv_sec < 0 ? INT64_MIN : INT64_MAX;
    }
    tz_os_put_i64(record + 16, nanoseconds);
    return tz_os_deliver(output, record, sizeof record);
}

static int64_t tz_os_environment(struct tz_os_buffer *output, const char *name) {
    if (name[0] == '\0' || strchr(name, '=') != NULL) return tz_os_invalid_input();
    const char *value = getenv(name);
    if (value == NULL) return tz_os_status(TZ_OS_NOT_FOUND, 0);
    return tz_os_deliver(output, (const unsigned char *)value, strlen(value));
}

// op 0 reads a file, 1 lists a directory, 2 returns the working directory (the path is ignored),
// 3 reads an environment variable (a missing one is kind 1 with code 0), and 4 and 5 describe a
// path, following a symbolic link (4) or not (5).
TZ_OS_API int64_t tsuzuri_os_read(struct tz_os_buffer *output, int32_t operation, const unsigned char *path, int64_t length) {
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

static int tz_os_argument_count = 0;
static char **tz_os_arguments = NULL;

TZ_OS_API void tsuzuri_os_set_args(int32_t count, char **arguments) {
    tz_os_argument_count = count;
    tz_os_arguments = arguments;
}

// The arguments after the program name, each followed by a NUL. Without a saved argv it is empty.
TZ_OS_API int64_t tsuzuri_os_args(struct tz_os_buffer *output) {
    tz_os_clear(output);
    size_t total = 0;
    for (int index = 1; index < tz_os_argument_count; index++) total += strlen(tz_os_arguments[index]) + 1;
    if (total == 0) return 0;
    unsigned char *joined = malloc(total);
    if (joined == NULL) return tz_os_status(TZ_OS_OTHER, ENOMEM);
    size_t offset = 0;
    for (int index = 1; index < tz_os_argument_count; index++) {
        size_t length = strlen(tz_os_arguments[index]) + 1;
        memcpy(joined + offset, tz_os_arguments[index], length);
        offset += length;
    }
    int64_t status = tz_os_deliver(output, joined, total);
    free(joined);
    return status;
}

static int64_t tz_os_write_file(const char *name, int flags, const unsigned char *data, int64_t length) {
    int descriptor = tz_os_open(name, flags);
    if (descriptor < 0) return tz_os_error(errno);
    int64_t offset = 0;
    while (offset < length) {
        ssize_t count = write(descriptor, data + offset, (size_t)(length - offset));
        if (count < 0) {
            if (errno == EINTR) continue;
            int error = errno;
            close(descriptor);
            return tz_os_error(error);
        }
        offset += count;
    }
    // After EINTR the descriptor state is unspecified, and retrying could close another one.
    if (close(descriptor) != 0 && errno != EINTR) return tz_os_error(errno);
    return 0;
}

// op 0 creates or truncates and writes, 1 creates or appends, 2 makes a directory (parents are
// not created), 3 removes an empty directory, and 4 unlinks a file or a symbolic link.
TZ_OS_API int64_t tsuzuri_os_write(int32_t operation, const unsigned char *path, int64_t path_length, const unsigned char *data, int64_t data_length) {
    int64_t status = 0;
    char *name = tz_os_name(path, path_length, &status);
    if (name == NULL) return status;
    switch (operation) {
    case 0:
        status = tz_os_write_file(name, O_WRONLY | O_CREAT | O_TRUNC, data, data_length);
        break;
    case 1:
        status = tz_os_write_file(name, O_WRONLY | O_CREAT | O_APPEND, data, data_length);
        break;
    case 2:
        status = mkdir(name, 0777) == 0 ? 0 : tz_os_error(errno);
        break;
    case 3:
        status = rmdir(name) == 0 ? 0 : tz_os_error(errno);
        break;
    case 4:
        status = unlink(name) == 0 ? 0 : tz_os_error(errno);
        break;
    default:
        status = tz_os_invalid_input();
        break;
    }
    free(name);
    return status;
}

// Open files (File.Handle). A handle is (generation << 32) | (slot + 1). Every open file gets a
// generation that is never used again, so a closed or stale handle is rejected instead of reaching
// whichever file now has the same descriptor number. The table is freed when no file is open.
struct tz_os_file {
    int descriptor;
    int64_t generation;
    int readable;
    int writable;
};

static struct tz_os_file *tz_os_files = NULL;
static size_t tz_os_file_capacity = 0;
static size_t tz_os_files_open = 0;
static int64_t tz_os_generation = 0;

static struct tz_os_file *tz_os_file_of(int64_t handle) {
    if (handle <= 0) return NULL;
    uint64_t slot = (uint64_t)handle & UINT64_C(0xffffffff);
    int64_t generation = handle >> 32;
    if (slot == 0 || slot > tz_os_file_capacity) return NULL;
    struct tz_os_file *file = &tz_os_files[slot - 1];
    return file->generation == generation && generation != 0 ? file : NULL;
}

static int64_t tz_os_bad_handle(void) {
    return tz_os_status(TZ_OS_INVALID_INPUT, EBADF);
}

// op 0 opens for reading, 1 creates or truncates for writing, 2 creates or appends, and 3 creates a
// new file and fails when it exists. A failure returns the negated status.
TZ_OS_API int64_t tsuzuri_os_open(int32_t mode, const unsigned char *path, int64_t length) {
    int64_t status = 0;
    char *name = tz_os_name(path, length, &status);
    if (name == NULL) return -status;
    int flags = 0, readable = 0, writable = 1;
    switch (mode) {
    case 0:
        flags = O_RDONLY;
        readable = 1;
        writable = 0;
        break;
    case 1:
        flags = O_WRONLY | O_CREAT | O_TRUNC;
        break;
    case 2:
        flags = O_WRONLY | O_CREAT | O_APPEND;
        break;
    case 3:
        flags = O_WRONLY | O_CREAT | O_EXCL;
        break;
    default:
        free(name);
        return -tz_os_invalid_input();
    }
    int descriptor = tz_os_open(name, flags);
    free(name);
    if (descriptor < 0) return -tz_os_error(errno);
    struct stat information;
    if (fstat(descriptor, &information) != 0 || S_ISDIR(information.st_mode)) {
        int error = S_ISDIR(information.st_mode) ? EISDIR : errno;
        close(descriptor);
        return -tz_os_error(error);
    }
    if (tz_os_generation >= INT32_MAX) {
        close(descriptor);
        return -tz_os_status(TZ_OS_OTHER, EOVERFLOW);
    }
    size_t slot = 0;
    while (slot < tz_os_file_capacity && tz_os_files[slot].generation != 0) slot++;
    if (slot == tz_os_file_capacity) {
        size_t next = tz_os_file_capacity == 0 ? 8 : tz_os_file_capacity * 2;
        if (next > UINT32_MAX) {
            close(descriptor);
            return -tz_os_status(TZ_OS_OTHER, EMFILE);
        }
        struct tz_os_file *larger = realloc(tz_os_files, next * sizeof(struct tz_os_file));
        if (larger == NULL) {
            close(descriptor);
            return -tz_os_status(TZ_OS_OTHER, ENOMEM);
        }
        for (size_t index = tz_os_file_capacity; index < next; index++) larger[index].generation = 0;
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

// op 0 reads up to `count` bytes (none at the end of the file), 1 writes `data` completely, and
// 2 flushes, which only checks the handle because nothing is buffered.
TZ_OS_API int64_t tsuzuri_os_handle(struct tz_os_buffer *output, int32_t operation, int64_t handle, int64_t count, const unsigned char *data, int64_t data_length) {
    tz_os_clear(output);
    struct tz_os_file *file = tz_os_file_of(handle);
    if (file == NULL) return tz_os_bad_handle();
    switch (operation) {
    case 0: {
        if (!file->readable) return tz_os_bad_handle();
        if (count < 0 || count > TZ_OS_MAX_RANDOM) return tz_os_invalid_input();
        if (count == 0) return 0;
        unsigned char *bytes = tsuzuri_alloc(count);
        for (;;) {
            ssize_t received = read(file->descriptor, bytes, (size_t)count);
            if (received < 0) {
                if (errno == EINTR) continue;
                int error = errno;
                tsuzuri_free(bytes);
                return tz_os_error(error);
            }
            if (received == 0) {
                tsuzuri_free(bytes);
                return 0;
            }
            output->data = bytes;
            output->length = (int64_t)received;
            return 0;
        }
    }
    case 1: {
        if (!file->writable) return tz_os_bad_handle();
        int64_t offset = 0;
        while (offset < data_length) {
            ssize_t count_written = write(file->descriptor, data + offset, (size_t)(data_length - offset));
            if (count_written < 0) {
                if (errno == EINTR) continue;
                return tz_os_error(errno);
            }
            offset += count_written;
        }
        return 0;
    }
    case 2:
        return 0;
    default:
        return tz_os_invalid_input();
    }
}

// Closes a file; the handle is invalid afterwards even when the system reports a failure.
TZ_OS_API int64_t tsuzuri_os_close(int64_t handle) {
    struct tz_os_file *file = tz_os_file_of(handle);
    if (file == NULL) return tz_os_bad_handle();
    int result = close(file->descriptor);
    int error = errno;
    file->generation = 0;
    if (--tz_os_files_open == 0) {
        free(tz_os_files);
        tz_os_files = NULL;
        tz_os_file_capacity = 0;
    }
    // After EINTR the descriptor state is unspecified, and retrying could close another one.
    return result != 0 && error != EINTR ? tz_os_error(error) : 0;
}

// Output captured from a child stays below this many bytes in all, or the child is killed.
#define TZ_OS_MAX_CAPTURE (INT64_C(1) << 30)

struct tz_os_capture {
    int descriptor;
    unsigned char *data;
    size_t length;
    size_t capacity;
};

// Reads what a child's pipe has, until it would block or `allowed` more bytes are in, so a child that keeps
// its pipe full cannot make this loop (and the buffer) grow past the budget. A false result is an
// allocation failure.
static int tz_os_drain(struct tz_os_capture *capture, size_t allowed) {
    while (allowed > 0) {
        if (capture->length == capture->capacity) {
            size_t next = capture->capacity == 0 ? 4096 : capture->capacity * 2;
            if (next - capture->length > allowed) next = capture->length + allowed;
            unsigned char *larger = realloc(capture->data, next);
            if (larger == NULL) return 0;
            capture->data = larger;
            capture->capacity = next;
        }
        size_t room = capture->capacity - capture->length;
        if (room > allowed) room = allowed;
        ssize_t count = read(capture->descriptor, capture->data + capture->length, room);
        if (count > 0) {
            capture->length += (size_t)count;
            allowed -= (size_t)count;
            continue;
        }
        if (count == 0) {
            close(capture->descriptor);
            capture->descriptor = -1;
            return 1;
        }
        if (errno == EINTR) continue;
        if (errno == EAGAIN || errno == EWOULDBLOCK) return 1;
        close(capture->descriptor);
        capture->descriptor = -1;
        return 1;
    }
    return 1;
}

static void tz_os_close_pipe(int pipe_ends[2]) {
    if (pipe_ends[0] >= 0) close(pipe_ends[0]);
    if (pipe_ends[1] >= 0) close(pipe_ends[1]);
}

static int tz_os_make_pipe(int pipe_ends[2]) {
    pipe_ends[0] = pipe_ends[1] = -1;
    if (pipe(pipe_ends) != 0) return 0;
    for (int index = 0; index < 2; index++) {
        if (fcntl(pipe_ends[index], F_SETFD, FD_CLOEXEC) != 0) {
            tz_os_close_pipe(pipe_ends);
            pipe_ends[0] = pipe_ends[1] = -1;
            return 0;
        }
    }
    return 1;
}

// Runs a program without a shell: `arguments` is the NUL-terminated argument list after the program name,
// `input` goes to its standard input, which is then closed. The result is two little-endian i32 (the exit
// code, or -1 and then the signal that ended it), two little-endian u64 (the lengths of the output and the
// error stream), and the two streams.
TZ_OS_API int64_t tsuzuri_os_spawn(struct tz_os_buffer *output, const unsigned char *program, int64_t program_length, const unsigned char *arguments, int64_t arguments_length, const unsigned char *input, int64_t input_length) {
    tz_os_clear(output);
    int64_t status = 0;
    char *name = tz_os_name(program, program_length, &status);
    if (name == NULL) return status;
    if (name[0] == '\0' || arguments_length < 0 || input_length < 0) {
        free(name);
        return tz_os_invalid_input();
    }
    size_t count = 0;
    for (int64_t index = 0; index < arguments_length; index++) {
        if (arguments[index] == 0) count++;
    }
    char *copy = malloc((size_t)arguments_length + 1);
    char **argv = calloc(count + 2, sizeof(char *));
    if (copy == NULL || argv == NULL) {
        free(copy);
        free(argv);
        free(name);
        return tz_os_status(TZ_OS_OTHER, ENOMEM);
    }
    if (arguments_length > 0) memcpy(copy, arguments, (size_t)arguments_length);
    copy[arguments_length] = '\0';
    argv[0] = name;
    size_t start = 0, position = 1;
    for (int64_t index = 0; index < arguments_length; index++) {
        if (arguments[index] == 0) {
            argv[position++] = copy + start;
            start = (size_t)index + 1;
        }
    }
    int standard_input[2] = {-1, -1}, standard_output[2] = {-1, -1}, standard_error[2] = {-1, -1};
    posix_spawn_file_actions_t actions;
    pid_t child = 0;
    int error = 0;
    int pipes_ready = tz_os_make_pipe(standard_input);
    pipes_ready = pipes_ready && tz_os_make_pipe(standard_output);
    pipes_ready = pipes_ready && tz_os_make_pipe(standard_error);
    if (!pipes_ready) {
        error = errno;
        tz_os_close_pipe(standard_input);
        tz_os_close_pipe(standard_output);
        tz_os_close_pipe(standard_error);
        free(copy);
        free(argv);
        free(name);
        return tz_os_error(error);
    }
    error = posix_spawn_file_actions_init(&actions);
    if (error == 0) {
        error = posix_spawn_file_actions_adddup2(&actions, standard_input[0], 0);
        if (error == 0) error = posix_spawn_file_actions_adddup2(&actions, standard_output[1], 1);
        if (error == 0) error = posix_spawn_file_actions_adddup2(&actions, standard_error[1], 2);
        if (error == 0) error = posix_spawnp(&child, name, &actions, NULL, argv, TZ_OS_ENVIRON);
        posix_spawn_file_actions_destroy(&actions);
    }
    close(standard_input[0]);
    close(standard_output[1]);
    close(standard_error[1]);
    free(copy);
    free(argv);
    free(name);
    if (error != 0) {
        close(standard_input[1]);
        close(standard_output[0]);
        close(standard_error[0]);
        return tz_os_error(error);
    }
    // A child that stops reading its input must not kill this process with SIGPIPE.
    struct sigaction ignore, previous;
    memset(&ignore, 0, sizeof ignore);
    ignore.sa_handler = SIG_IGN;
    sigemptyset(&ignore.sa_mask);
    sigaction(SIGPIPE, &ignore, &previous);
    int writing = standard_input[1];
    size_t written = 0;
    struct tz_os_capture streams[2] = {{standard_output[0], NULL, 0, 0}, {standard_error[0], NULL, 0, 0}};
    fcntl(writing, F_SETFL, fcntl(writing, F_GETFL) | O_NONBLOCK);
    for (int index = 0; index < 2; index++) fcntl(streams[index].descriptor, F_SETFL, fcntl(streams[index].descriptor, F_GETFL) | O_NONBLOCK);
    if (input_length == 0) {
        close(writing);
        writing = -1;
    }
    int failed = 0, too_much = 0;
    while (writing >= 0 || streams[0].descriptor >= 0 || streams[1].descriptor >= 0) {
        struct pollfd waiting[3];
        int kinds[3], total = 0;
        if (writing >= 0) {
            waiting[total].fd = writing;
            waiting[total].events = POLLOUT;
            kinds[total++] = 2;
        }
        for (int index = 0; index < 2; index++) {
            if (streams[index].descriptor < 0) continue;
            waiting[total].fd = streams[index].descriptor;
            waiting[total].events = POLLIN;
            kinds[total++] = index;
        }
        if (poll(waiting, (nfds_t)total, -1) < 0) {
            if (errno == EINTR) continue;
            failed = errno;
            break;
        }
        for (int index = 0; index < total; index++) {
            if (waiting[index].revents == 0) continue;
            if (kinds[index] == 2) {
                size_t chunk = (size_t)input_length - written;
                if (chunk > 65536) chunk = 65536;
                ssize_t count_written = write(writing, input + written, chunk);
                if (count_written > 0) written += (size_t)count_written;
                // The child closing its input ends the writing; nothing is lost that it would have read.
                if ((count_written < 0 && errno != EINTR && errno != EAGAIN && errno != EWOULDBLOCK) || written == (size_t)input_length) {
                    close(writing);
                    writing = -1;
                }
            } else {
                // A read stops one byte past the limit, which the check after this loop treats as too much.
                size_t allowed = (size_t)TZ_OS_MAX_CAPTURE + 1 - streams[0].length - streams[1].length;
                if (!tz_os_drain(&streams[kinds[index]], allowed)) failed = ENOMEM;
            }
        }
        if (failed) break;
        if ((int64_t)(streams[0].length + streams[1].length) > TZ_OS_MAX_CAPTURE) {
            too_much = 1;
            break;
        }
    }
    sigaction(SIGPIPE, &previous, NULL);
    if (failed || too_much) kill(child, SIGKILL);
    if (writing >= 0) close(writing);
    for (int index = 0; index < 2; index++) {
        if (streams[index].descriptor >= 0) close(streams[index].descriptor);
    }
    int wait_status = 0;
    while (waitpid(child, &wait_status, 0) < 0) {
        if (errno != EINTR) {
            wait_status = 0;
            if (!failed) failed = errno;
            break;
        }
    }
    int64_t result = 0;
    if (failed || too_much) {
        result = tz_os_status(TZ_OS_OTHER, too_much ? EFBIG : failed);
    } else {
        int code = WIFEXITED(wait_status) ? WEXITSTATUS(wait_status) : -1;
        int signal_number = WIFSIGNALED(wait_status) ? WTERMSIG(wait_status) : 0;
        size_t total = 24 + streams[0].length + streams[1].length;
        unsigned char *bytes = tsuzuri_alloc((int64_t)total);
        for (int index = 0; index < 4; index++) {
            bytes[index] = (unsigned char)((uint32_t)code >> (8 * index));
            bytes[4 + index] = (unsigned char)((uint32_t)signal_number >> (8 * index));
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

TZ_OS_API int64_t tsuzuri_os_random(struct tz_os_buffer *output, int64_t count) {
    tz_os_clear(output);
    if (count < 0 || count > TZ_OS_MAX_RANDOM) return tz_os_invalid_input();
    if (count == 0) return 0;
    unsigned char *bytes = tsuzuri_alloc(count);
    for (int64_t offset = 0; offset < count;) {
        size_t chunk = (size_t)(count - offset < 256 ? count - offset : 256);
        if (getentropy(bytes + offset, chunk) != 0) {
            int error = errno;
            tsuzuri_free(bytes);
            return tz_os_error(error);
        }
        offset += (int64_t)chunk;
    }
    output->data = bytes;
    output->length = count;
    return 0;
}

// op 0 is the monotonic clock and 1 the Unix clock, both in nanoseconds; INT64_MIN reports failure.
TZ_OS_API int64_t tsuzuri_os_clock(int32_t operation) {
    struct timespec now;
    if (clock_gettime(operation == 0 ? CLOCK_MONOTONIC : CLOCK_REALTIME, &now) != 0) return INT64_MIN;
    if (now.tv_sec < 0 || now.tv_sec > (INT64_MAX - now.tv_nsec) / INT64_C(1000000000)) return INT64_MIN;
    return (int64_t)now.tv_sec * INT64_C(1000000000) + (int64_t)now.tv_nsec;
}

TZ_OS_API int64_t tsuzuri_os_sleep(int64_t milliseconds) {
    if (milliseconds < 0) return tz_os_invalid_input();
    if (milliseconds == 0) return 0;
    struct timespec remaining;
    remaining.tv_sec = (time_t)(milliseconds / 1000);
    remaining.tv_nsec = (long)(milliseconds % 1000) * 1000000L;
    while (nanosleep(&remaining, &remaining) != 0) {
        if (errno != EINTR) return tz_os_error(errno);
    }
    return 0;
}
