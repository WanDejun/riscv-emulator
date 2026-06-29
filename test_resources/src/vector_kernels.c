#include "io.h"
#include "power.h"
#include "rvv_test.h"
#include <stddef.h>
#include <stdint.h>

#define ARRAY_LEN(x) (sizeof(x) / sizeof((x)[0]))

static int check_i32(const char* name, const int32_t* got, const int32_t* expected, size_t n) {
    for (size_t i = 0; i < n; ++i) {
        if (got[i] != expected[i]) {
            printf("%s mismatch at %ld: got %ld expected %ld\n", name, (long)i, (long)got[i],
                   (long)expected[i]);
            return 0;
        }
    }
    return 1;
}

static int check_i16(const char* name, const int16_t* got, const int16_t* expected, size_t n) {
    for (size_t i = 0; i < n; ++i) {
        if (got[i] != expected[i]) {
            printf("%s mismatch at %ld: got %ld expected %ld\n", name, (long)i, (long)got[i],
                   (long)expected[i]);
            return 0;
        }
    }
    return 1;
}

static int check_scalar(const char* name, long got, long expected) {
    if (got != expected) {
        printf("%s mismatch: got %ld expected %ld\n", name, got, expected);
        return 0;
    }
    return 1;
}

static void saxpy_ref(int32_t* dst, const int32_t* x, const int32_t* y, int32_t a, size_t n) {
    for (size_t i = 0; i < n; ++i) {
        dst[i] = a * x[i] + y[i];
    }
}

static int run_saxpy(void) {
    enum { N = 31 };
    static int32_t x[N];
    static int32_t y[N];
    static int32_t got[N];
    static int32_t expected[N];

    for (size_t i = 0; i < N; ++i) {
        x[i] = (int32_t)i - 13;
        y[i] = (int32_t)(i * 3) - 7;
    }

    saxpy_ref(expected, x, y, -5, N);
    rvv_saxpy_i32(got, x, y, -5, N);
    return check_i32("saxpy_i32", got, expected, N);
}

static void gemm_ref(int32_t* c, const int32_t* a, const int32_t* b, size_t m, size_t n,
                     size_t k) {
    for (size_t i = 0; i < m; ++i) {
        for (size_t j = 0; j < n; ++j) {
            int32_t acc = 0;
            for (size_t p = 0; p < k; ++p) {
                acc += a[i * k + p] * b[p * n + j];
            }
            c[i * n + j] = acc;
        }
    }
}

static void gemm_rvv_i32(int32_t* c, const int32_t* a, const int32_t* b, size_t m, size_t n,
                         size_t k) {
    static int32_t b_col[16];
    static int32_t tmp[16];

    for (size_t i = 0; i < m; ++i) {
        for (size_t j = 0; j < n; ++j) {
            for (size_t p = 0; p < k; ++p) {
                b_col[p] = b[p * n + j];
            }

            rvv_dot_i32(tmp, &a[i * k], b_col, k);

            int32_t acc = 0;
            for (size_t p = 0; p < k; ++p) {
                acc += tmp[p];
            }
            c[i * n + j] = acc;
        }
    }
}

static int run_gemm(void) {
    enum { M = 4, N = 5, K = 6 };
    static int32_t a[M * K];
    static int32_t b[K * N];
    static int32_t got[M * N];
    static int32_t expected[M * N];

    for (size_t i = 0; i < ARRAY_LEN(a); ++i) {
        a[i] = (int32_t)(i % 9) - 4;
    }
    for (size_t i = 0; i < ARRAY_LEN(b); ++i) {
        b[i] = (int32_t)((i * 5) % 11) - 5;
    }

    gemm_ref(expected, a, b, M, N, K);
    gemm_rvv_i32(got, a, b, M, N, K);
    return check_i32("gemm_i32", got, expected, ARRAY_LEN(got));
}

static void conv1d_ref(int32_t* dst, const int32_t* src, const int32_t* kernel, size_t out_len,
                       size_t kernel_len) {
    for (size_t i = 0; i < out_len; ++i) {
        int32_t acc = 0;
        for (size_t k = 0; k < kernel_len; ++k) {
            acc += src[i + k] * kernel[k];
        }
        dst[i] = acc;
    }
}

static void conv1d_rvv_i32(int32_t* dst, const int32_t* src, const int32_t* kernel,
                           size_t out_len, size_t kernel_len) {
    static int32_t tmp[16];
    for (size_t i = 0; i < out_len; ++i) {
        rvv_dot_i32(tmp, &src[i], kernel, kernel_len);
        int32_t acc = 0;
        for (size_t k = 0; k < kernel_len; ++k) {
            acc += tmp[k];
        }
        dst[i] = acc;
    }
}

static int run_conv(void) {
    enum { IN = 17, K = 5, OUT = IN - K + 1 };
    static int32_t src[IN];
    static int32_t kernel[K] = {3, -2, 1, 4, -1};
    static int32_t got[OUT];
    static int32_t expected[OUT];

    for (size_t i = 0; i < IN; ++i) {
        src[i] = (int32_t)((i * i + 3) % 17) - 8;
    }

    conv1d_ref(expected, src, kernel, OUT, K);
    conv1d_rvv_i32(got, src, kernel, OUT, K);
    return check_i32("conv1d_i32", got, expected, OUT);
}

static void conv2d3x3_ref(int32_t* dst, const int32_t* src, const int32_t* kernel, size_t in_h,
                          size_t in_w) {
    const size_t out_h = in_h - 2;
    const size_t out_w = in_w - 2;
    for (size_t oh = 0; oh < out_h; ++oh) {
        for (size_t ow = 0; ow < out_w; ++ow) {
            int32_t acc = 0;
            for (size_t kh = 0; kh < 3; ++kh) {
                for (size_t kw = 0; kw < 3; ++kw) {
                    acc += src[(oh + kh) * in_w + ow + kw] * kernel[kh * 3 + kw];
                }
            }
            dst[oh * out_w + ow] = acc;
        }
    }
}

static void conv2d3x3_rvv_i32(int32_t* dst, const int32_t* src, const int32_t* kernel,
                              size_t in_h, size_t in_w) {
    static int32_t patch[9];
    static int32_t tmp[9];
    const size_t out_h = in_h - 2;
    const size_t out_w = in_w - 2;

    for (size_t oh = 0; oh < out_h; ++oh) {
        for (size_t ow = 0; ow < out_w; ++ow) {
            for (size_t kh = 0; kh < 3; ++kh) {
                for (size_t kw = 0; kw < 3; ++kw) {
                    patch[kh * 3 + kw] = src[(oh + kh) * in_w + ow + kw];
                }
            }

            rvv_dot_i32(tmp, patch, kernel, 9);

            int32_t acc = 0;
            for (size_t i = 0; i < 9; ++i) {
                acc += tmp[i];
            }
            dst[oh * out_w + ow] = acc;
        }
    }
}

static int run_conv2d(void) {
    enum { H = 5, W = 6, OH = H - 2, OW = W - 2 };
    static int32_t src[H * W];
    static int32_t kernel[9] = {1, 0, -1, 2, -2, 1, 3, 1, -3};
    static int32_t got[OH * OW];
    static int32_t expected[OH * OW];

    for (size_t i = 0; i < ARRAY_LEN(src); ++i) {
        src[i] = (int32_t)((i * 7 + 5) % 19) - 9;
    }

    conv2d3x3_ref(expected, src, kernel, H, W);
    conv2d3x3_rvv_i32(got, src, kernel, H, W);
    return check_i32("conv2d3x3_i32", got, expected, ARRAY_LEN(got));
}

static int run_instruction_smoke(void) {
    enum { N = 8 };
    static int32_t a[N] = {-9, -4, -1, 0, 1, 2, 7, 11};
    static int32_t b[N] = {3, -2, 5, 7, -9, 4, 7, 6};
    static uint32_t index[N] = {7, 6, 5, 4, 3, 2, 1, 0};
    static int32_t got[N];
    static int32_t expected[N];
    static int16_t got16[N];
    static int16_t expected16[N];
    long first = -2;
    long count = -1;

    rvv_add_vv_i32(got, a, b, N);
    for (size_t i = 0; i < N; ++i) expected[i] = a[i] + b[i];
    if (!check_i32("vadd.vv", got, expected, N)) return 0;

    rvv_sub_vv_i32(got, a, b, N);
    for (size_t i = 0; i < N; ++i) expected[i] = a[i] - b[i];
    if (!check_i32("vsub.vv", got, expected, N)) return 0;

    rvv_and_vv_i32(got, a, b, N);
    for (size_t i = 0; i < N; ++i) expected[i] = a[i] & b[i];
    if (!check_i32("vand.vv", got, expected, N)) return 0;

    rvv_or_vv_i32(got, a, b, N);
    for (size_t i = 0; i < N; ++i) expected[i] = a[i] | b[i];
    if (!check_i32("vor.vv", got, expected, N)) return 0;

    rvv_xor_vv_i32(got, a, b, N);
    for (size_t i = 0; i < N; ++i) expected[i] = a[i] ^ b[i];
    if (!check_i32("vxor.vv", got, expected, N)) return 0;

    rvv_sll1_vi_i32(got, a, N);
    for (size_t i = 0; i < N; ++i) expected[i] = a[i] << 1;
    if (!check_i32("vsll.vi", got, expected, N)) return 0;

    rvv_sra1_vi_i32(got, a, N);
    for (size_t i = 0; i < N; ++i) expected[i] = a[i] >> 1;
    if (!check_i32("vsra.vi", got, expected, N)) return 0;

    count = rvv_mseq_count_first_i32(a, b, N, &first);
    if (!check_scalar("vmseq.vv/vcpop.m", count, 1)) return 0;
    if (!check_scalar("vmseq.vv/vfirst.m", first, 6)) return 0;

    rvv_merge_eq_i32(got, a, b, N);
    for (size_t i = 0; i < N; ++i) expected[i] = (i == 6) ? a[i] : b[i];
    if (!check_i32("vmerge.vvm", got, expected, N)) return 0;

    rvv_gather_i32(got, a, index, N);
    for (size_t i = 0; i < N; ++i) expected[i] = a[index[i]];
    if (!check_i32("vrgather.vv", got, expected, N)) return 0;

    rvv_slideup1_i32(got, a, N);
    expected[0] = 0;
    for (size_t i = 1; i < N; ++i) expected[i] = a[i - 1];
    if (!check_i32("vslideup.vi", got, expected, N)) return 0;

    rvv_slidedown1_i32(got, a, N);
    for (size_t i = 0; i + 1 < N; ++i) expected[i] = a[i + 1];
    expected[N - 1] = 0;
    if (!check_i32("vslidedown.vi", got, expected, N)) return 0;

    rvv_narrow_shift_i32_to_i16_m2(got16, a, 1, N);
    for (size_t i = 0; i < N; ++i) expected16[i] = (int16_t)(a[i] >> 1);
    if (!check_i16("vnsra.wx", got16, expected16, N)) return 0;

    rvv_move_scalar_pair_i32(got, a, 12345);
    if (!check_scalar("vmv.s.x/vmv.x.s", got[0], 12345)) return 0;

    return 1;
}

int main(void) {
    TEST_START(__BASE_FILE__);
    rvv_enable();

    if (!run_instruction_smoke()) fail();
    if (!run_saxpy()) fail();
    if (!run_gemm()) fail();
    if (!run_conv()) fail();
    if (!run_conv2d()) fail();

    printf("vector kernels ok\n");
    pass();
    return 0;
}
