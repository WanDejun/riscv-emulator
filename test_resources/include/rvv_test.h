#pragma once

#include <stddef.h>
#include <stdint.h>

static inline void rvv_enable(void) {
    uintptr_t mstatus;
    asm volatile("csrr %0, mstatus" : "=r"(mstatus));
    mstatus |= (uintptr_t)0x3 << 9;
    asm volatile("csrw mstatus, %0" : : "r"(mstatus) : "memory");
}

static inline size_t rvv_vsetvl_e8m1(size_t avl) {
    size_t vl;
    asm volatile("vsetvli %0, %1, e8, m1, tu, mu" : "=r"(vl) : "r"(avl));
    return vl;
}

static inline size_t rvv_vsetvl_e16m1(size_t avl) {
    size_t vl;
    asm volatile("vsetvli %0, %1, e16, m1, tu, mu" : "=r"(vl) : "r"(avl));
    return vl;
}

static inline size_t rvv_vsetvl_e32m1(size_t avl) {
    size_t vl;
    asm volatile("vsetvli %0, %1, e32, m1, tu, mu" : "=r"(vl) : "r"(avl));
    return vl;
}

static inline size_t rvv_vsetvl_e64m1(size_t avl) {
    size_t vl;
    asm volatile("vsetvli %0, %1, e64, m1, tu, mu" : "=r"(vl) : "r"(avl));
    return vl;
}

static inline void rvv_load_i32_v8(const int32_t* src) {
    asm volatile("vle32.v v8, (%0)" : : "r"(src) : "memory");
}

static inline void rvv_load_i32_v16(const int32_t* src) {
    asm volatile("vle32.v v16, (%0)" : : "r"(src) : "memory");
}

static inline void rvv_load_i32_v24(const int32_t* src) {
    asm volatile("vle32.v v24, (%0)" : : "r"(src) : "memory");
}

static inline void rvv_store_i32_v8(int32_t* dst) {
    asm volatile("vse32.v v8, (%0)" : : "r"(dst) : "memory");
}

static inline void rvv_store_i32_v24(int32_t* dst) {
    asm volatile("vse32.v v24, (%0)" : : "r"(dst) : "memory");
}

static inline void rvv_load_i16_v8(const int16_t* src) {
    asm volatile("vle16.v v8, (%0)" : : "r"(src) : "memory");
}

static inline void rvv_store_i16_v8(int16_t* dst) {
    asm volatile("vse16.v v8, (%0)" : : "r"(dst) : "memory");
}

static inline void rvv_load_i8_v8(const int8_t* src) {
    asm volatile("vle8.v v8, (%0)" : : "r"(src) : "memory");
}

static inline void rvv_store_i8_v8(int8_t* dst) {
    asm volatile("vse8.v v8, (%0)" : : "r"(dst) : "memory");
}

#define RVV_WRAP_VV_I32(name, asm_name)                                                  \
    static inline void rvv_##name##_vv_i32(int32_t* dst, const int32_t* lhs,             \
                                           const int32_t* rhs, size_t n) {               \
        while (n) {                                                                       \
            size_t vl = rvv_vsetvl_e32m1(n);                                             \
            rvv_load_i32_v16(lhs);                                                       \
            rvv_load_i32_v8(rhs);                                                        \
            asm volatile(asm_name " v24, v16, v8" ::: "memory");                        \
            rvv_store_i32_v24(dst);                                                      \
            lhs += vl;                                                                    \
            rhs += vl;                                                                    \
            dst += vl;                                                                    \
            n -= vl;                                                                      \
        }                                                                                 \
    }

#define RVV_WRAP_VX_I32(name, asm_name)                                                  \
    static inline void rvv_##name##_vx_i32(int32_t* dst, const int32_t* src, int32_t x,  \
                                           size_t n) {                                   \
        while (n) {                                                                       \
            size_t vl = rvv_vsetvl_e32m1(n);                                             \
            rvv_load_i32_v8(src);                                                        \
            asm volatile(asm_name " v24, v8, %0" : : "r"(x) : "memory");                \
            rvv_store_i32_v24(dst);                                                      \
            src += vl;                                                                    \
            dst += vl;                                                                    \
            n -= vl;                                                                      \
        }                                                                                 \
    }

#define RVV_WRAP_VI_I32(name, asm_name, imm)                                             \
    static inline void rvv_##name##_vi_i32(int32_t* dst, const int32_t* src, size_t n) { \
        while (n) {                                                                       \
            size_t vl = rvv_vsetvl_e32m1(n);                                             \
            rvv_load_i32_v8(src);                                                        \
            asm volatile(asm_name " v24, v8, " #imm ::: "memory");                      \
            rvv_store_i32_v24(dst);                                                      \
            src += vl;                                                                    \
            dst += vl;                                                                    \
            n -= vl;                                                                      \
        }                                                                                 \
    }

RVV_WRAP_VV_I32(add, "vadd.vv")
RVV_WRAP_VV_I32(sub, "vsub.vv")
RVV_WRAP_VV_I32(mul, "vmul.vv")
RVV_WRAP_VV_I32(and, "vand.vv")
RVV_WRAP_VV_I32(or, "vor.vv")
RVV_WRAP_VV_I32(xor, "vxor.vv")
RVV_WRAP_VV_I32(sll, "vsll.vv")
RVV_WRAP_VV_I32(srl, "vsrl.vv")
RVV_WRAP_VV_I32(sra, "vsra.vv")
RVV_WRAP_VV_I32(min, "vmin.vv")
RVV_WRAP_VV_I32(max, "vmax.vv")
RVV_WRAP_VV_I32(minu, "vminu.vv")
RVV_WRAP_VV_I32(maxu, "vmaxu.vv")

RVV_WRAP_VX_I32(add, "vadd.vx")
RVV_WRAP_VX_I32(sub, "vsub.vx")
RVV_WRAP_VX_I32(rsub, "vrsub.vx")
RVV_WRAP_VX_I32(mul, "vmul.vx")
RVV_WRAP_VX_I32(and, "vand.vx")
RVV_WRAP_VX_I32(or, "vor.vx")
RVV_WRAP_VX_I32(xor, "vxor.vx")
RVV_WRAP_VX_I32(sll, "vsll.vx")
RVV_WRAP_VX_I32(srl, "vsrl.vx")
RVV_WRAP_VX_I32(sra, "vsra.vx")

RVV_WRAP_VI_I32(add3, "vadd.vi", 3)
RVV_WRAP_VI_I32(and7, "vand.vi", 7)
RVV_WRAP_VI_I32(or5, "vor.vi", 5)
RVV_WRAP_VI_I32(xor6, "vxor.vi", 6)
RVV_WRAP_VI_I32(sll1, "vsll.vi", 1)
RVV_WRAP_VI_I32(srl1, "vsrl.vi", 1)
RVV_WRAP_VI_I32(sra1, "vsra.vi", 1)

#define RVV_WRAP_MASK_VV_I32(name, asm_name)                                             \
    static inline void rvv_##name##_vv_i32(uint8_t* mask_out, const int32_t* lhs,        \
                                           const int32_t* rhs, size_t n) {               \
        while (n) {                                                                       \
            size_t vl = rvv_vsetvl_e32m1(n);                                             \
            rvv_load_i32_v16(lhs);                                                       \
            rvv_load_i32_v8(rhs);                                                        \
            asm volatile(asm_name " v0, v16, v8\n\tvsm.v v0, (%0)"                      \
                         :                                                               \
                         : "r"(mask_out)                                                \
                         : "memory");                                                   \
            lhs += vl;                                                                    \
            rhs += vl;                                                                    \
            mask_out += (vl + 7) / 8;                                                    \
            n -= vl;                                                                      \
        }                                                                                 \
    }

/* These helpers materialize mask registers with vsm.v. They are useful for
 * ISA coverage once mask load/store is available in the emulator. The runnable
 * smoke tests below use register-local mask producers instead. */
RVV_WRAP_MASK_VV_I32(mseq, "vmseq.vv")
RVV_WRAP_MASK_VV_I32(msne, "vmsne.vv")
RVV_WRAP_MASK_VV_I32(mslt, "vmslt.vv")
RVV_WRAP_MASK_VV_I32(msle, "vmsle.vv")
RVV_WRAP_MASK_VV_I32(msltu, "vmsltu.vv")
RVV_WRAP_MASK_VV_I32(msleu, "vmsleu.vv")

static inline void rvv_merge_vvm_i32(int32_t* dst, const int32_t* a, const int32_t* b,
                                     const uint8_t* mask, size_t n) {
    while (n) {
        size_t vl = rvv_vsetvl_e32m1(n);
        rvv_load_i32_v16(a);
        rvv_load_i32_v8(b);
        asm volatile("vlm.v v0, (%0)\n\tvmerge.vvm v24, v8, v16, v0" : : "r"(mask) : "memory");
        rvv_store_i32_v24(dst);
        a += vl;
        b += vl;
        dst += vl;
        mask += (vl + 7) / 8;
        n -= vl;
    }
}

static inline void rvv_merge_eq_i32(int32_t* dst, const int32_t* a, const int32_t* b, size_t n) {
    while (n) {
        size_t vl = rvv_vsetvl_e32m1(n);
        rvv_load_i32_v16(a);
        rvv_load_i32_v8(b);
        asm volatile("vmseq.vv v0, v16, v8\n\tvmerge.vvm v24, v8, v16, v0" ::: "memory");
        rvv_store_i32_v24(dst);
        a += vl;
        b += vl;
        dst += vl;
        n -= vl;
    }
}

static inline long rvv_mseq_count_first_i32(const int32_t* lhs, const int32_t* rhs, size_t n,
                                            long* first_out) {
    long total = 0;
    long first = -1;
    long offset = 0;

    while (n) {
        long count;
        long first_in_chunk;
        size_t vl = rvv_vsetvl_e32m1(n);
        rvv_load_i32_v16(lhs);
        rvv_load_i32_v8(rhs);
        asm volatile("vmseq.vv v0, v16, v8\n\tvcpop.m %0, v0\n\tvfirst.m %1, v0"
                     : "=r"(count), "=r"(first_in_chunk)
                     :
                     : "memory");
        if (first < 0 && first_in_chunk >= 0) {
            first = offset + first_in_chunk;
        }
        total += count;
        offset += (long)vl;
        lhs += vl;
        rhs += vl;
        n -= vl;
    }

    *first_out = first;
    return total;
}

static inline void rvv_saxpy_i32(int32_t* dst, const int32_t* x, const int32_t* y, int32_t a,
                                 size_t n) {
    while (n) {
        size_t vl = rvv_vsetvl_e32m1(n);
        rvv_load_i32_v8(x);
        rvv_load_i32_v16(y);
        asm volatile("vmul.vx v8, v8, %0\n\tvadd.vv v24, v8, v16" : : "r"(a) : "memory");
        rvv_store_i32_v24(dst);
        x += vl;
        y += vl;
        dst += vl;
        n -= vl;
    }
}

static inline void rvv_dot_i32(int32_t* tmp, const int32_t* a, const int32_t* b, size_t n) {
    while (n) {
        size_t vl = rvv_vsetvl_e32m1(n);
        rvv_load_i32_v8(a);
        rvv_load_i32_v16(b);
        asm volatile("vmul.vv v24, v8, v16" ::: "memory");
        rvv_store_i32_v24(tmp);
        a += vl;
        b += vl;
        tmp += vl;
        n -= vl;
    }
}

static inline void rvv_slideup1_i32(int32_t* dst, const int32_t* src, size_t n) {
    size_t vl = rvv_vsetvl_e32m1(n);
    rvv_load_i32_v8(src);
    asm volatile("vmv.v.i v24, 0\n\tvslideup.vi v24, v8, 1" ::: "memory");
    rvv_store_i32_v24(dst);
}

static inline void rvv_slidedown1_i32(int32_t* dst, const int32_t* src, size_t n) {
    size_t vl = rvv_vsetvl_e32m1(n);
    rvv_load_i32_v8(src);
    asm volatile("vslidedown.vi v24, v8, 1" ::: "memory");
    rvv_store_i32_v24(dst);
}

static inline void rvv_gather_i32(int32_t* dst, const int32_t* src, const uint32_t* index,
                                  size_t n) {
    while (n) {
        size_t vl = rvv_vsetvl_e32m1(n);
        rvv_load_i32_v8(src);
        asm volatile("vle32.v v16, (%0)\n\tvrgather.vv v24, v8, v16" : : "r"(index) : "memory");
        rvv_store_i32_v24(dst);
        dst += vl;
        index += vl;
        n -= vl;
    }
}

static inline void rvv_narrow_shift_i32_to_i16_m2(int16_t* dst, const int32_t* src, uint32_t shift,
                                                  size_t n) {
    while (n) {
        size_t vl;
        asm volatile("vsetvli %0, %3, e16, m1, tu, mu\n\t"
                     "vle32.v v8, (%1)\n\t"
                     "vnsra.wx v24, v8, %2\n\t"
                     "vse16.v v24, (%4)"
                     : "=&r"(vl)
                     : "r"(src), "r"(shift), "r"(n), "r"(dst)
                     : "memory");
        src += vl;
        dst += vl;
        n -= vl;
    }
}

static inline void rvv_move_scalar_pair_i32(int32_t* dst, const int32_t* src, int32_t value) {
    int32_t out;
    rvv_vsetvl_e32m1(1);
    rvv_load_i32_v8(src);
    asm volatile("vmv.s.x v8, %1\n\tvmv.x.s %0, v8" : "=r"(out) : "r"(value) : "memory");
    asm volatile("sw %0, 0(%1)"
                 :
                 : "r"(out), "r"(dst)
                 : "memory");
}

static inline long rvv_mask_cpop_first(const uint8_t* mask, size_t n, long* first_out) {
    long count;
    long first;
    rvv_vsetvl_e8m1(n);
    asm volatile("vlm.v v8, (%2)\n\tvcpop.m %0, v8\n\tvfirst.m %1, v8"
                 : "=r"(count), "=r"(first)
                 : "r"(mask)
                 : "memory");
    *first_out = first;
    return count;
}
