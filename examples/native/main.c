#include <math.h>
#include <stdio.h>
#include "physics.h"

int main(void) {
    const double positions[] = { 9.0, 1.0 };
    const double velocities[] = { 3.0, -3.0 };
    tsuzuri_f64_buffer advanced;
    tz_next_positions(&advanced, positions, 2, velocities, 2, 1.0, 10.0);
    const int valid = advanced.len == 2 && advanced.ptr[0] == 8.0 && advanced.ptr[1] == 2.0;
    tsuzuri_free(advanced.ptr);
    if (!valid) return 1;
    double x = 100.0, y = 50.0, vx = 120.0, vy = 90.0;
    for (int frame = 0; frame < 600; ++frame) {
        const double next_x = tz_next_position(x, vx, 1.0 / 60.0, 640.0);
        const double next_y = tz_next_position(y, vy, 1.0 / 60.0, 360.0);
        const double next_vx = tz_next_velocity(x, vx, 1.0 / 60.0, 640.0);
        const double next_vy = tz_next_velocity(y, vy, 1.0 / 60.0, 360.0);
        x = next_x;
        y = next_y;
        vx = next_vx;
        vy = next_vy;
        if (!(x >= 0.0 && x <= 640.0 && y >= 0.0 && y <= 360.0
              && fabs(vx) == 120.0 && fabs(vy) == 90.0)) {
            fputs("physics invariant failed\n", stderr);
            return 1;
        }
    }
    printf("{\"x\":%.17g,\"y\":%.17g,\"vx\":%.17g,\"vy\":%.17g}\n", x, y, vx, vy);
    return 0;
}
