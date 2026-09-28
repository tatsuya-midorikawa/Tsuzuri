#include <errno.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

#if defined(_WIN32)
#include <fcntl.h>
#include <io.h>
#define TZ_IO_API
#else
#define TZ_IO_API __attribute__((weak, visibility("hidden")))
#endif

extern void *tsuzuri_alloc(int64_t size);
extern void tsuzuri_free(void *pointer);

struct tz_io_buffer {
    unsigned char *data;
    int64_t length;
};

TZ_IO_API int32_t tsuzuri_io_read_line(struct tz_io_buffer *output) {
    output->data = NULL;
    output->length = 0;
#if defined(_WIN32)
    if (_setmode(0, _O_BINARY) == -1) return 2;
#endif
    int64_t capacity = 0;
    for (;;) {
        errno = 0;
        int value = fgetc(stdin);
        if (value == EOF) {
            if (ferror(stdin)) {
                if (errno == EINTR) {
                    clearerr(stdin);
                    continue;
                }
                tsuzuri_free(output->data);
                output->data = NULL;
                output->length = 0;
                return 2;
            }
            return output->length == 0 ? 1 : 0;
        }
        if (value == '\n') {
            if (output->length > 0 && output->data[output->length - 1] == '\r') {
                output->length--;
            }
            return 0;
        }
        if (output->length == capacity) {
            if (capacity > INT64_MAX / 2) {
                tsuzuri_free(output->data);
                output->data = NULL;
                output->length = 0;
                return 2;
            }
            int64_t next_capacity = capacity == 0 ? 256 : capacity * 2;
            unsigned char *next = tsuzuri_alloc(next_capacity);
            if (output->length > 0) memcpy(next, output->data, (size_t)output->length);
            tsuzuri_free(output->data);
            output->data = next;
            capacity = next_capacity;
        }
        output->data[output->length++] = (unsigned char)value;
    }
}

TZ_IO_API int32_t tsuzuri_io_write(int32_t destination, const unsigned char *data, int64_t length) {
    if ((destination != 1 && destination != 2) || length < 0) return 1;
#if defined(_WIN32)
    if (_setmode(destination, _O_BINARY) == -1) return 1;
#endif
    FILE *stream = destination == 1 ? stdout : stderr;
    int64_t offset = 0;
    while (offset < length) {
        size_t written = fwrite(data + offset, 1, (size_t)(length - offset), stream);
        offset += (int64_t)written;
        if (ferror(stream) || written == 0) return 1;
    }
    return fflush(stream) == 0 ? 0 : 1;
}
