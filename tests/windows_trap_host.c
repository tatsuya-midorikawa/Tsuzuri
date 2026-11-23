/* W07: C host linking a Tsuzuri `--emit object` (`tz_on_tick`) without a Tsuzuri VEH.
 * Mode "timer": invokes `tz_on_tick(0)` inside SetTimer's TIMERPROC (where DispatchMessageW wraps
 * callbacks in an SEH filter unless fail-fast is used).
 * Mode "create": invokes `tz_on_tick(0)` inside WM_CREATE (kernel callback frame). */
#undef NDEBUG
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#ifndef UNICODE
#define UNICODE
#endif
#include <windows.h>
#include <stdint.h>
#include <string.h>

extern int64_t tz_on_tick(int64_t divisor);

static int g_mode_create = 0;

static VOID CALLBACK host_timer_proc(HWND hwnd, UINT msg, UINT_PTR id, DWORD time) {
    (void)hwnd;
    (void)msg;
    (void)id;
    (void)time;
    (void)tz_on_tick(0);
    PostQuitMessage(0);
}

static LRESULT CALLBACK host_wndproc(HWND hwnd, UINT msg, WPARAM wparam, LPARAM lparam) {
    if (msg == WM_CREATE && g_mode_create) {
        (void)tz_on_tick(0);
        return 0;
    }
    return DefWindowProcW(hwnd, msg, wparam, lparam);
}

int main(int argc, char **argv) {
    g_mode_create = (argc > 1 && strcmp(argv[1], "create") == 0);
    WNDCLASSEXW wc;
    ZeroMemory(&wc, sizeof(wc));
    wc.cbSize = sizeof(wc);
    wc.lpfnWndProc = host_wndproc;
    wc.hInstance = GetModuleHandleW(NULL);
    wc.lpszClassName = L"tz-trap-host";
    if (!RegisterClassExW(&wc) && GetLastError() != ERROR_CLASS_ALREADY_EXISTS) return 2;

    HWND hwnd = CreateWindowExW(0, L"tz-trap-host", L"host", 0, 0, 0, 0, 0, HWND_MESSAGE, NULL, GetModuleHandleW(NULL), NULL);
    if (!hwnd) return 3;
    if (g_mode_create) {
        DestroyWindow(hwnd);
        return 77;
    }

    UINT_PTR guard = SetTimer(NULL, 0, 5000, NULL);
    SetTimer(hwnd, 1, 10, host_timer_proc);
    MSG msg;
    while (GetMessageW(&msg, NULL, 0, 0) > 0) {
        if (msg.hwnd == NULL && msg.message == WM_TIMER && msg.wParam == guard) {
            DestroyWindow(hwnd);
            return 4;
        }
        TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }
    if (guard) KillTimer(NULL, guard);
    if (IsWindow(hwnd)) DestroyWindow(hwnd);
    return 0;
}
