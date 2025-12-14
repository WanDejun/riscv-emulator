#include "io.h"
#include "power.h"
#include <stdint.h>

const double eps = 1e-9;

double sqrt(double x) {
    double l = 0, r = x;
    while (r - l > eps) {
        double mid = (l + r) / 2;
        if (mid * mid < x) {
            l = mid;
        } else {
            r = mid;
        }
    }
    return (l + r) / 2;
}

int main() {
    /// Enable FPU by setting FS field in mstatus to 11
    asm volatile (
        "li t0, (3 << 13)\n"
        "csrs mstatus, t0\n"
        ::: "t0"
    );

    printf("Square root of 2 is %.7f\n", sqrt(2));

    PowerOff();

    return 0;
}
