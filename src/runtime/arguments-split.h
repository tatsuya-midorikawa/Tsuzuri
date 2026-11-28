// Shared Windows command-line token splitter used by both src/runtime/arguments.c (def main) and
// src/runtime/os-windows.h (Env.args) so both report the exact same argument array (W11 D4, W33).
// Follows UCRT (Visual Studio 2008 and later) wmain argv[1..] rules.
#ifndef TZ_ARGUMENTS_SPLIT_DEFINED
#define TZ_ARGUMENTS_SPLIT_DEFINED

static const uint16_t *tz_arguments_blank(const uint16_t *at) {
    while (*at == ' ' || *at == '\t') at++;
    return at;
}

// The program name (UCRT): read from the very first unit without skipping leading blanks; a quote
// toggles quoting and is removed, a backslash is an ordinary character, and a space or a tab
// outside quotes ends it. A line that begins with a blank has an empty program name, so its first
// word is an argument.
static const uint16_t *tz_arguments_program(const uint16_t *at) {
    int quoted = 0;
    for (; *at != 0 && (quoted || (*at != ' ' && *at != '\t')); at++) {
        if (*at == '"') quoted = !quoted;
    }
    return at;
}

// One argument (UCRT, Visual Studio 2008 and later): 2n backslashes and a quote are n backslashes
// and a quote that opens or closes quoting; 2n + 1 backslashes and a quote are n backslashes and a
// literal quote; other backslashes are literal; inside quotes, "" is one literal quote and quoting
// continues.
static const uint16_t *tz_arguments_token(const uint16_t *at, uint16_t *output, int64_t *length) {
    int quoted = 0;
    int64_t count = 0;
    for (;;) {
        int64_t slashes = 0;
        while (*at == '\\') {
            at++;
            slashes++;
        }
        int copy = 1;
        if (*at == '"') {
            if (slashes % 2 == 0) {
                if (quoted && at[1] == '"') {
                    at++; /* "" inside quotes: the second is a literal quote */
                } else {
                    copy = 0;
                    quoted = !quoted;
                }
            }
            slashes /= 2;
        }
        for (; slashes > 0; slashes--) {
            if (output) output[count] = '\\';
            count++;
        }
        if (*at == 0 || (!quoted && (*at == ' ' || *at == '\t'))) break;
        if (copy) {
            if (output) output[count] = *at;
            count++;
        }
        at++;
    }
    *length = count;
    return at;
}

#endif
