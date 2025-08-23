#include "io.h"
#include "power.h"
#include <stdint.h>

int is_prime(int n) {
    if (n <= 1)
        return 0;
    for (int i = 2; i * i <= n; ++i) {
        if (n % i == 0)
            return 0;
    }
    return 1;
}

int main() {
    int count = 0;
    for (int i = 2; i < 20000; ++i) {
        if (is_prime(i))
            count++;
    }
    // print count
    printf("%d", count);

    PowerOff();
    return 0;
}
