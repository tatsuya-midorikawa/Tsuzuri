// Shared Windows command-line token splitter used by both src/runtime/arguments.c (def main) and
// src/runtime/os-windows.h (Env.args) so both report the exact same argument array (W11 D4).
#ifndef TZ_ARGUMENTS_SPLIT_DEFINED
#define TZ_ARGUMENTS_SPLIT_DEFINED

static const uint16_t *tz_arguments_blank(const uint16_t *at) {
    while (*at == ' ' || *at == '\t') at++;
    return at;
}

// Reads the argument at `at`, stores its units in `output` unless it is NULL, and returns where
// it ends.
static const uint16_t *tz_arguments_token(const uint16_t *at, uint16_t *output, int64_t *length) {
    int quoted = 0;
    int64_t count = 0;
    for (; *at != 0 && (quoted || (*at != ' ' && *at != '\t')); at++) {
        if (*at == '"') {
            quoted = !quoted;
        } else {
            if (output) output[count] = *at;
            count++;
        }
    }
    *length = count;
    return at;
}

#endif
