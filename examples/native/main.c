#include <math.h>
#include <stdio.h>
#include "physics.h"

int main(void) {
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
