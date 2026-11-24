/* W06: Headless Win32 message-only window (`HWND_MESSAGE`) test shim.
 * Never calls tested functions inside `assert(...)` because `zig cc` defines `NDEBUG` at `-O1+`. */
#undef NDEBUG
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#ifndef UNICODE
#define UNICODE
#endif
#include <windows.h>
#include <process.h>
#include <stdint.h>
#include <stdlib.h>

typedef int64_t (*tz_msg_cb)(uint32_t message, uint64_t wparam, int64_t lparam);

typedef struct tz_pump_state {
    tz_msg_cb callback;
    int64_t count;
    int64_t expected;
    int timed_out;
} tz_pump_state;

typedef struct tz_poster_arg {
    HWND hwnd;
    WPARAM wparam;
} tz_poster_arg;

static tz_msg_cb g_create_callback = NULL;
static int64_t g_create_result = 0;

static LRESULT CALLBACK tz_test_wndproc(HWND hwnd, UINT msg, WPARAM wparam, LPARAM lparam) {
    if (msg == WM_NCCREATE) {
        CREATESTRUCTW *cs = (CREATESTRUCTW *)lparam;
        if (cs && cs->lpCreateParams) {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, (LONG_PTR)cs->lpCreateParams);
        }
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }
    if (msg == WM_CREATE && g_create_callback != NULL) {
        tz_msg_cb cb = g_create_callback;
        g_create_callback = NULL;
        g_create_result = cb((uint32_t)msg, (uint64_t)wparam, (int64_t)(intptr_t)hwnd);
        return 0;
    }
    tz_pump_state *state = (tz_pump_state *)GetWindowLongPtrW(hwnd, GWLP_USERDATA);
    if (state && state->callback) {
        if (msg == WM_TIMER || msg == WM_APP || msg == (WM_APP + 1)) {
            if (msg == WM_TIMER) {
                KillTimer(hwnd, (UINT_PTR)wparam);
            }
            int64_t r = state->callback((uint32_t)msg, (uint64_t)wparam, (int64_t)lparam);
            if (msg == (WM_APP + 1)) {
                return (LRESULT)r;
            }
            state->count += r;
            if (state->expected > 0 && state->count >= state->expected) {
                DestroyWindow(hwnd);
            }
            return (LRESULT)r;
        }
    }
    if (msg == WM_DESTROY && state != NULL) {
        PostQuitMessage(0);
        return 0;
    }
    return DefWindowProcW(hwnd, msg, wparam, lparam);
}

static int ensure_test_class(void) {
    WNDCLASSEXW wc;
    ZeroMemory(&wc, sizeof(wc));
    wc.cbSize = sizeof(wc);
    wc.lpfnWndProc = tz_test_wndproc;
    wc.hInstance = GetModuleHandleW(NULL);
    wc.lpszClassName = L"tz-test";
    if (!RegisterClassExW(&wc)) {
        DWORD err = GetLastError();
        if (err != ERROR_CLASS_ALREADY_EXISTS) return 0;
    }
    return 1;
}

static unsigned __stdcall post_worker(void *param) {
    tz_poster_arg *arg = (tz_poster_arg *)param;
    Sleep(15);
    (void)PostMessageW(arg->hwnd, WM_APP, arg->wparam, (LPARAM)(intptr_t)arg->hwnd);
    free(arg);
    return 0;
}

int64_t w06_gui_pump(tz_msg_cb on_message, int64_t expected) {
    if (!ensure_test_class()) return -2;
    tz_pump_state state;
    state.callback = on_message;
    state.count = 0;
    state.expected = expected;
    state.timed_out = 0;

    HWND hwnd = CreateWindowExW(0, L"tz-test", L"pump", 0, 0, 0, 0, 0, HWND_MESSAGE, NULL, GetModuleHandleW(NULL), &state);
    if (!hwnd) return -3;

    UINT_PTR guard = SetTimer(NULL, 0, 5000, NULL);
    SetTimer(hwnd, 1, 10, NULL);
    SetTimer(hwnd, 2, 15, NULL);

    tz_poster_arg *arg = (tz_poster_arg *)malloc(sizeof(tz_poster_arg));
    if (!arg) {
        if (guard) KillTimer(NULL, guard);
        DestroyWindow(hwnd);
        return -4;
    }
    arg->hwnd = hwnd;
    arg->wparam = 7;
    HANDLE worker = (HANDLE)_beginthreadex(NULL, 0, post_worker, arg, 0, NULL);
    if (!worker) free(arg);

    MSG msg;
    while (GetMessageW(&msg, NULL, 0, 0) > 0) {
        if (msg.hwnd == NULL && msg.message == WM_TIMER && msg.wParam == guard) {
            state.timed_out = 1;
            if (IsWindow(hwnd)) DestroyWindow(hwnd);
            break;
        }
        TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }
    if (guard) KillTimer(NULL, guard);
    if (worker) {
        WaitForSingleObject(worker, 5000);
        CloseHandle(worker);
    }
    return state.timed_out ? -1 : state.count;
}

int64_t w06_gui_send(int64_t window, uint32_t message, uint64_t wparam, int64_t lparam) {
    return (int64_t)SendMessageW((HWND)(intptr_t)window, (UINT)message, (WPARAM)wparam, (LPARAM)lparam);
}

int64_t w06_gui_window(void) {
    if (!ensure_test_class()) return 0;
    HWND hwnd = CreateWindowExW(0, L"tz-test", L"wait", 0, 0, 0, 0, 0, HWND_MESSAGE, NULL, GetModuleHandleW(NULL), NULL);
    return (int64_t)(intptr_t)hwnd;
}

int64_t w13_gui_sized_window(int32_t width, int32_t height) {
    if (!ensure_test_class()) return 0;
    HWND hwnd = CreateWindowExW(0, L"tz-test", L"sized", WS_POPUP, 0, 0, width, height, NULL, NULL, GetModuleHandleW(NULL), NULL);
    return (int64_t)(intptr_t)hwnd;
}

int64_t w06_gui_wait(int64_t window) {
    HWND hwnd = (HWND)(intptr_t)window;
    if (!hwnd) return -2;
    UINT_PTR guard = SetTimer(NULL, 0, 5000, NULL);
    int64_t result = -1;
    MSG msg;
    while (GetMessageW(&msg, NULL, 0, 0) > 0) {
        if (msg.hwnd == NULL && msg.message == WM_TIMER && msg.wParam == guard) {
            result = -1;
            break;
        }
        if (msg.hwnd == hwnd && msg.message == WM_APP) {
            result = (int64_t)msg.wParam;
            break;
        }
        TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }
    if (guard) KillTimer(NULL, guard);
    if (IsWindow(hwnd)) DestroyWindow(hwnd);
    return result;
}

int64_t w06_gui_post(int64_t window, uint64_t wparam) {
    BOOL ok = PostMessageW((HWND)(intptr_t)window, WM_APP, (WPARAM)wparam, 0);
    return ok ? 1 : 0;
}

int64_t w06_gui_create(tz_msg_cb on_create) {
    if (!ensure_test_class()) return -2;
    g_create_callback = on_create;
    g_create_result = -1;
    HWND hwnd = CreateWindowExW(0, L"tz-test", L"create", 0, 0, 0, 0, 0, HWND_MESSAGE, NULL, GetModuleHandleW(NULL), NULL);
    g_create_callback = NULL;
    if (hwnd) DestroyWindow(hwnd);
    return g_create_result;
}

void w06_gui_mark(const uint16_t *path, int64_t length) {
    if (!path || length <= 0 || length >= 4096) return;
    WCHAR wide[4096];
    for (int64_t i = 0; i < length; i++) {
        wide[i] = (WCHAR)path[i];
    }
    wide[length] = 0;
    HANDLE file = CreateFileW(wide, GENERIC_WRITE, FILE_SHARE_READ | FILE_SHARE_WRITE, NULL, CREATE_ALWAYS, FILE_ATTRIBUTE_NORMAL, NULL);
    if (file != INVALID_HANDLE_VALUE) {
        DWORD written = 0;
        (void)WriteFile(file, "marked\n", 7, &written, NULL);
        CloseHandle(file);
    }
}

/* W07 / W12: runs a SetTimer TIMERPROC callback `expected` times on a message-only window. */
static int64_t (*g_timer_proc_cb)(uint64_t id) = NULL;
static int64_t g_timer_proc_count = 0;
static int64_t g_timer_proc_expected = 0;
static HWND g_timer_proc_hwnd = NULL;

static VOID CALLBACK tz_shim_timer_proc(HWND hwnd, UINT msg, UINT_PTR id, DWORD time) {
    (void)msg;
    (void)time;
    if (g_timer_proc_cb != NULL) {
        int64_t delta = g_timer_proc_cb((uint64_t)id);
        g_timer_proc_count += delta;
        if (g_timer_proc_expected > 0 && g_timer_proc_count >= g_timer_proc_expected) {
            KillTimer(hwnd, id);
            if (IsWindow(hwnd)) DestroyWindow(hwnd);
            PostQuitMessage(0);
        }
    }
}

int64_t w07_gui_timer_proc(int64_t (*on_tick)(uint64_t id), int64_t expected) {
    if (!ensure_test_class()) return -2;
    HWND hwnd = CreateWindowExW(0, L"tz-test", L"timerproc", 0, 0, 0, 0, 0, HWND_MESSAGE, NULL, GetModuleHandleW(NULL), NULL);
    if (!hwnd) return -3;
    g_timer_proc_cb = on_tick;
    g_timer_proc_count = 0;
    g_timer_proc_expected = expected;
    g_timer_proc_hwnd = hwnd;

    UINT_PTR guard = SetTimer(NULL, 0, 5000, NULL);
    SetTimer(hwnd, 9, 10, tz_shim_timer_proc);

    int timed_out = 0;
    MSG msg;
    while (GetMessageW(&msg, NULL, 0, 0) > 0) {
        if (msg.hwnd == NULL && msg.message == WM_TIMER && msg.wParam == guard) {
            timed_out = 1;
            break;
        }
        TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }
    if (guard) KillTimer(NULL, guard);
    g_timer_proc_cb = NULL;
    if (IsWindow(hwnd)) DestroyWindow(hwnd);
    return timed_out ? -1 : g_timer_proc_count;
}

/* W12 / W22 / W23: direct WNDPROC registration in WNDCLASSEXW, Windows thread-pool callback, and atexit. */
typedef int64_t (*w12_wndproc_t)(int64_t hwnd, uint32_t msg, uint64_t wparam, int64_t lparam);
static int g_w12_class_seq = 0;
static WCHAR g_w12_class_name[64];

int64_t w12_gui_register_class(w12_wndproc_t proc) {
    int id = ++g_w12_class_seq;
    wsprintfW(g_w12_class_name, L"tz-w12-%d-%lu", id, (unsigned long)GetCurrentProcessId());
    WNDCLASSEXW wc;
    ZeroMemory(&wc, sizeof(wc));
    wc.cbSize = sizeof(wc);
    wc.lpfnWndProc = (WNDPROC)proc;
    wc.hInstance = GetModuleHandleW(NULL);
    wc.lpszClassName = g_w12_class_name;
    return RegisterClassExW(&wc) ? 1 : 0;
}

int64_t w22_gui_register_class(int64_t tag, w12_wndproc_t proc) {
    if (w12_gui_register_class(proc) == 0) return 0;
    return tag;
}

int64_t w12_gui_create_registered(void) {
    HWND hwnd = CreateWindowExW(0, g_w12_class_name, L"w12", 0, 0, 0, 0, 0, HWND_MESSAGE, NULL, GetModuleHandleW(NULL), NULL);
    return (int64_t)(intptr_t)hwnd;
}

int64_t w12_gui_def_proc(int64_t hwnd, uint32_t msg, uint64_t wparam, int64_t lparam) {
    return (int64_t)DefWindowProcW((HWND)(intptr_t)hwnd, (UINT)msg, (WPARAM)wparam, (LPARAM)lparam);
}

int64_t w12_gui_destroy(int64_t hwnd) {
    return DestroyWindow((HWND)(intptr_t)hwnd) ? 1 : 0;
}

int64_t w12_gui_pump_pending(void) {
    int64_t count = 0;
    MSG msg;
    while (PeekMessageW(&msg, NULL, 0, 0, PM_REMOVE)) {
        if (msg.message == WM_QUIT) break;
        TranslateMessage(&msg);
        DispatchMessageW(&msg);
        count++;
    }
    return count;
}

int64_t w12_gui_set_userdata(int64_t hwnd, int64_t value) {
    return (int64_t)SetWindowLongPtrW((HWND)(intptr_t)hwnd, GWLP_USERDATA, (LONG_PTR)value);
}

int64_t w12_gui_get_userdata(int64_t hwnd) {
    return (int64_t)GetWindowLongPtrW((HWND)(intptr_t)hwnd, GWLP_USERDATA);
}

static LONG volatile g_w12_mark = 0;
void w12_gui_set_mark(int64_t value) {
    InterlockedExchange(&g_w12_mark, (LONG)value);
}
int64_t w12_gui_get_mark(void) {
    return (int64_t)InterlockedCompareExchange(&g_w12_mark, 0, 0);
}

typedef struct {
    int64_t (*cb)(int64_t);
    int64_t arg;
    int64_t out;
    HANDLE done;
} w12_tp_ctx;

static VOID NTAPI w12_tp_callback(PTP_CALLBACK_INSTANCE instance, PVOID context) {
    (void)instance;
    w12_tp_ctx *ctx = (w12_tp_ctx *)context;
    int64_t r = ctx->cb(ctx->arg);
    int64_t m = w12_gui_get_mark();
    ctx->out = r + m;
    SetEvent(ctx->done);
}

int64_t w12_gui_pool_submit_wait(int64_t (*cb)(int64_t), int64_t arg) {
    w12_gui_set_mark(0);
    HANDLE ev = CreateEventW(NULL, TRUE, FALSE, NULL);
    if (!ev) return -1;
    w12_tp_ctx ctx = { cb, arg, 0, ev };
    if (!TrySubmitThreadpoolCallback(w12_tp_callback, &ctx, NULL)) {
        CloseHandle(ev);
        return -2;
    }
    DWORD waited = WaitForSingleObject(ev, 5000);
    CloseHandle(ev);
    return waited == WAIT_OBJECT_0 ? ctx.out : -3;
}

static int64_t (*g_w12_after_main_cb)(int64_t) = NULL;
static void w12_atexit_runner(void) {
    if (g_w12_after_main_cb) (void)g_w12_after_main_cb(9);
}
void w12_gui_call_after_main(int64_t (*cb)(int64_t)) {
    g_w12_after_main_cb = cb;
    atexit(w12_atexit_runner);
}

static volatile LONG g_w12_idle_ready = 0;
static unsigned __stdcall w12_idle_worker(void *param) {
    int64_t (*cb)(int64_t) = (int64_t (*)(int64_t))param;
    (void)cb(1);
    InterlockedExchange(&g_w12_idle_ready, 1);
    Sleep(5000);
    return 0;
}
void w12_gui_start_idle_thread(int64_t (*cb)(int64_t)) {
    InterlockedExchange(&g_w12_idle_ready, 0);
    HANDLE th = (HANDLE)_beginthreadex(NULL, 0, w12_idle_worker, (void *)cb, 0, NULL);
    if (th) CloseHandle(th);
    while (InterlockedCompareExchange(&g_w12_idle_ready, 0, 0) == 0) SwitchToThread();
}


