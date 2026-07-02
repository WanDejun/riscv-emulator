use std::{f32, thread};

use super::*;
use crate::{
    isa::riscv::{cpu_tester::*, csr_reg::csr_index, vector::VLEN},
    ram_config,
    utils::{UnsignedInteger, negative_of, sign_extend},
};

#[test]
fn test_exec_arith() {
    let mut tester = ExecTester::new();

    run_test_exec(
        RiscvInstr::ADDI,
        RVInstrInfo::I {
            rd: 2,
            rs1: 3,
            imm: negative_of(5),
        },
        |builder| builder.reg(3, 10).pc(0x2000),
        |checker| checker.reg(2, 5).pc(0x2004),
    );

    for _ in 1..=100 {
        tester.test_rand_r(RiscvInstr::ADD, |lhs, rhs| lhs.wrapping_add(rhs));
        tester.test_rand_r(RiscvInstr::SUB, |lhs, rhs| lhs.wrapping_sub(rhs));
        tester.test_rand_i(RiscvInstr::ADDI, |lhs, imm| lhs.wrapping_add(imm));

        tester.test_rand_i(RiscvInstr::SLTI, |lhs, imm| {
            ((lhs.cast_signed()) < (sign_extend(imm, 12).cast_signed())) as WordType
        });
        tester.test_rand_i(RiscvInstr::SLTIU, |lhs, imm| {
            ((lhs) < (sign_extend(imm, 12))) as WordType
        });
    }

    run_test_exec_decode(
        0x02520333, // mul x6, x4, x5
        |builder| builder.reg(4, 5).reg(5, 10).pc(0x1000),
        |checker| checker.reg(6, 50).pc(0x1004),
    );
}

#[test]
fn test_load_store_decode() {
    run_test_exec_decode(
        0x00812183, // lw x3, 8(x2)
        |builder| {
            builder
                .reg(2, ram_config::BASE_ADDR)
                .mem_base::<u32>(8, 123)
                .pc(0x1000)
        },
        |checker| checker.reg(3, 123).pc(0x1004),
    );

    run_test_exec_decode(
        0xfec42783, // lw a5,-20(s0)
        |builder| {
            builder
                .reg(8, ram_config::BASE_ADDR + 36)
                .mem_base(16, 123 as u32)
                .pc(0x1000)
        },
        |checker| checker.reg(15, 123).pc(0x1004),
    );

    run_test_exec_decode(
        0xfe112c23, // sw x1, -8(x2)
        |builder| builder.reg(2, ram_config::BASE_ADDR + 16).reg(1, 123),
        |checker| checker.mem_base::<u32>(8, 123),
    );

    run_test_exec_decode(
        0xfcf42e23, // sw a5,-36(s0)
        |builder| builder.reg(15, 123).reg(8, ram_config::BASE_ADDR + 72),
        |checker| checker.mem_base::<u32>(36, 123),
    );

    run_test_exec_decode(
        0xfef43423, // sd a5,-24(s0)
        |builder| builder.reg(15, 123).reg(8, ram_config::BASE_ADDR + 24),
        |checker| checker.mem_base::<u32>(0, 123),
    );
}

#[test]
fn test_u_types_decode() {
    run_test_exec_decode(
        0x12233097, // auipc x1, 0x112233
        |builder| builder.reg(1, 3).pc(0x1000),
        |checker| checker.reg(1, 0x12234000).pc(0x1004),
    );

    run_test_exec_decode(
        0x80000097, // auipc x1, 0x80000
        |builder| builder.pc(0x1000),
        |checker| checker.reg(1, 0xffffffff80001000).pc(0x1004),
    );

    run_test_exec_decode(
        0x123451b7, //lui x3, 0x12345
        |builder| builder.reg(3, 0x54321).pc(0x1000),
        |checker| checker.reg(3, 0x12345000).pc(0x1004),
    );
}

#[test]
fn test_branch_decode() {
    run_test_exec_decode(
        0xf8c318e3, // bne x6, x12, -112
        |builder| builder.reg(6, 5).reg(12, 10).pc(0x2000),
        |checker| checker.pc(0x2000 - 112),
    );
}

#[test]
fn test_jump_decode() {
    run_test_exec_decode(
        0xf81ff06f, // jal x0, -128
        |builder| builder.reg(0, 0).pc(0x1234),
        |checker| checker.pc(0x1234 - 128),
    );

    run_test_exec_decode(
        0x00078067, // jr a5
        |builder| builder.reg(15, 0x2468).pc(0x1234),
        |checker| checker.pc(0x2468),
    );
}

#[test]
fn test_csr() {
    // 2) CSRRS x12, mtvec(0x305), x6
    run_test_exec_decode(
        0x30532673,
        |builder| builder.reg(6, 0x00F0).csr(0x305, 0x0F00).pc(0x1000),
        |checker| checker.reg(12, 0x0F00).csr(0x305, 0x0FF0).pc(0x1004),
    );

    // 3) CSRRC x13, mepc(0x341), x7
    run_test_exec_decode(
        0x3413b6f3,
        |builder| builder.reg(7, 0x0FF0).csr(0x341, 0x0FFF).pc(0x1000),
        |checker| checker.reg(13, 0x0FFF).csr(0x341, 0x000F).pc(0x1004),
    );

    // 4) CSRRWI x11, mcause(0x342), imm=5
    run_test_exec_decode(
        0x3422d5f3,
        |builder| builder.csr(0x342, 0xABCD).pc(0x1000),
        |checker| checker.reg(11, 0xABCD).csr(0x342, 5).pc(0x1004),
    );

    // 5) CSRRSI x12, mip(0x344), imm=6
    run_test_exec_decode(
        0x34436673,
        |builder| builder.csr(0x344, 0x00F0).pc(0x1000),
        |checker| checker.reg(12, 0x00F0).csr(0x344, 0x00F6).pc(0x1004),
    );

    // 6) CSRRCI x13, mie(0x304), imm=7
    run_test_exec_decode(
        0x3043f6f3,
        |builder| builder.csr(0x304, 0x00FF).pc(0x1000),
        |checker| checker.reg(13, 0x00FF).csr(0x304, 0x00F8).pc(0x1004),
    );
}

#[test]
fn test_rv_m() {
    run_test_exec_decode(
        0x02c59733, // mulh a4,a1,a2
        |builder| builder.reg(11, 0xffffffffffff8000).reg(12, 0),
        |checker| checker.reg(14, 0),
    );

    run_test_exec_decode(
        0x02c59733, // mulh a4,a1,a2
        |builder| {
            builder
                .reg(11, 0xffffffff80000000)
                .reg(12, 0xffffffffffff8000)
        },
        |checker| checker.reg(14, 0),
    );
}

#[test]
fn test_rv32_f() {
    run_test_exec_decode(
        0x001015f3, // fsflags a1,zero => csrrw a1, fflags, zero
        |builder| builder.reg(1, 0).csr(3, 0b11011111),
        |checker| {
            checker
                .reg(1, 0)
                .reg(11, 0b11111)
                .csr(1, 0)
                .csr(2, 0b110)
                .csr(3, 0b11000000)
        },
    );

    run_test_exec_decode(
        0xe0068553, // fmv.x.w a0,fa3
        |builder| builder.reg_f32(13, 3.5),
        |checker| checker.reg(10, 0x40600000),
    );

    run_test_exec_decode(
        0x00b576d3, // fadd.s fa3,fa0,fa1
        |builder| builder.reg_f32(10, 3.14159265).reg_f32(11, 0.00000001),
        |checker| checker.reg_f32(13, 3.14159265).csr(3, 0b00001),
    );

    run_test_exec_decode(
        0x08b576d3, // fsub.s fa3,fa0,fa1
        |builder| {
            builder
                .reg_f32(10, f32::INFINITY)
                .reg_f32(11, f32::INFINITY)
        },
        |checker| checker.csr(3, 0b10000),
    );

    run_test_exec_decode(
        0x00102573, // frflags a0 => csrrs a0, fflags, x0
        |builder| builder.csr(csr_index::fcsr, 0b11011011),
        |checker| checker.reg(10, 0b11011),
    );

    run_test_exec_decode(
        0xd0057553, // fcvt.s.w fa0,a0
        |builder| builder.reg(10, negative_of(2)),
        |checker| checker.reg_f32(10, -2.0),
    );

    run_test_exec_decode(
        0xd0357553, // fcvt.s.lu fa0,a0
        |builder| builder.reg(10, 2),
        |checker| checker.reg_f32(10, 2.0),
    );

    run_test_exec_decode(
        0xc0051553, // fcvt.w.s a0,fa0,rtz
        |builder| builder.reg_f32(10, -1.1),
        |checker| checker.reg(10, negative_of(1)).csr(csr_index::fflags, 1),
    );

    run_test_exec_decode(
        0xc0051553, // fcvt.w.s a0,fa0,rtz
        |builder| builder.reg_f32(10, -1.0),
        |checker| checker.reg(10, negative_of(1)).csr(csr_index::fflags, 0),
    );

    // Cannot represent in dest format.
    run_test_exec_decode(
        0xc0051553, // fcvt.w.s a0,fa0,rtz
        |builder| builder.reg_f32(10, -3e9),
        |checker| {
            checker
                .reg(10, negative_of(1).wrapping_shl(31))
                .csr(csr_index::fflags, 0x10)
        },
    );

    // fcvt.w.s `-NAN`, should give i32::MAX
    run_test_exec_decode(
        0xc0051553, // fcvt.w.s a0,fa0,rtz
        |builder| builder.reg_f32(10, f32::from_bits(0xffffffff)),
        |checker| checker.reg(10, i32::MAX as WordType),
    );
}

#[test]
fn test_rv64_f() {
    run_test_exec_decode(
        0xe2068553, // fmv.x.d a0,fa3
        |builder| builder.reg_f64(13, 3.5),
        |checker| checker.reg(10, 3.5f64.to_bits()),
    );
}

#[test]
fn test_rv_c_arith() {
    run_test_exec(
        RiscvInstr::C_ADD,
        RVInstrInfo::CR { rd_rs1: 5, rs2: 6 },
        |builder| builder.reg(5, 10).reg(6, 20).pc(0x2000),
        |checker| checker.reg(5, 30).pc(0x2002), // compressed -> pc += 2
    );

    run_test_exec(
        RiscvInstr::C_ADDI,
        RVInstrInfo::CI {
            rd_rs1: 5,
            imm: negative_of(3),
        },
        |builder| builder.reg(5, 10).pc(0x2000),
        |checker| checker.reg(5, 7).pc(0x2002),
    );

    run_test_exec(
        RiscvInstr::C_SUB,
        RVInstrInfo::CA { rd_rs1: 8, rs2: 9 },
        |builder| builder.reg(8, 30).reg(9, 12).pc(0x2000),
        |checker| checker.reg(8, 18).pc(0x2002),
    );

    run_test_exec(
        RiscvInstr::C_AND,
        RVInstrInfo::CA { rd_rs1: 8, rs2: 9 },
        |builder| builder.reg(8, 0b1100).reg(9, 0b1010).pc(0x2000),
        |checker| checker.reg(8, 0b1000).pc(0x2002),
    );
    run_test_exec(
        RiscvInstr::C_OR,
        RVInstrInfo::CA { rd_rs1: 8, rs2: 9 },
        |builder| builder.reg(8, 0b1100).reg(9, 0b1010).pc(0x2000),
        |checker| checker.reg(8, 0b1110).pc(0x2002),
    );
    run_test_exec(
        RiscvInstr::C_XOR,
        RVInstrInfo::CA { rd_rs1: 8, rs2: 9 },
        |builder| builder.reg(8, 0b1100).reg(9, 0b1010).pc(0x2000),
        |checker| checker.reg(8, 0b0110).pc(0x2002),
    );

    run_test_exec(
        RiscvInstr::C_ANDI,
        RVInstrInfo::CB {
            rd_rs1: 8,
            imm: 0b110,
        },
        |builder| builder.reg(8, 0b1011).pc(0x2000),
        |checker| checker.reg(8, 0b0010).pc(0x2002),
    );

    run_test_exec(
        RiscvInstr::C_SLLI,
        RVInstrInfo::CI { rd_rs1: 5, imm: 4 },
        |builder| builder.reg(5, 1).pc(0x2000),
        |checker| checker.reg(5, 16).pc(0x2002),
    );
    run_test_exec(
        RiscvInstr::C_SRLI,
        RVInstrInfo::CB { rd_rs1: 8, imm: 1 },
        |builder| builder.reg(8, 0x10).pc(0x2000),
        |checker| checker.reg(8, 0x8).pc(0x2002),
    );
    run_test_exec(
        RiscvInstr::C_SRAI,
        RVInstrInfo::CB { rd_rs1: 8, imm: 1 },
        |builder| builder.reg(8, negative_of(16)).pc(0x2000),
        |checker| checker.reg(8, negative_of(8)).pc(0x2002),
    );

    run_test_exec(
        RiscvInstr::C_ADDW,
        RVInstrInfo::CA { rd_rs1: 8, rs2: 9 },
        |builder| builder.reg(8, 0x7fff_ffff).reg(9, 1).pc(0x2000),
        |checker| checker.reg(8, 0xffff_ffff_8000_0000).pc(0x2002),
    );
    run_test_exec(
        RiscvInstr::C_SUBW,
        RVInstrInfo::CA { rd_rs1: 8, rs2: 9 },
        |builder| builder.reg(8, 20).reg(9, 5).pc(0x2000),
        |checker| checker.reg(8, 15).pc(0x2002),
    );

    run_test_exec(
        RiscvInstr::C_ADDI16SP,
        RVInstrInfo::CI {
            rd_rs1: 2,
            imm: negative_of(16),
        },
        |builder| builder.reg(2, 0x100).pc(0x2000),
        |checker| checker.reg(2, 0xf0).pc(0x2002),
    );
    run_test_exec(
        RiscvInstr::C_ADDI4SPN,
        RVInstrInfo::CIW { rd: 8, imm: 16 },
        |builder| builder.reg(2, 0x100).pc(0x2000),
        |checker| checker.reg(8, 0x110).pc(0x2002),
    );

    run_test_exec(
        RiscvInstr::C_MV,
        RVInstrInfo::CR { rd_rs1: 5, rs2: 6 },
        |builder| builder.reg(6, 0xabc).pc(0x2000),
        |checker| checker.reg(5, 0xabc).pc(0x2002),
    );

    run_test_exec(
        RiscvInstr::C_LI,
        RVInstrInfo::CI {
            rd_rs1: 5,
            imm: negative_of(7),
        },
        |builder| builder.reg(5, 0xdead).pc(0x2000),
        |checker| checker.reg(5, negative_of(7)).pc(0x2002),
    );
    run_test_exec(
        RiscvInstr::C_LUI,
        RVInstrInfo::CI {
            rd_rs1: 5,
            imm: 0x12345 << 12,
        },
        |builder| builder.reg(5, 0).pc(0x2000),
        |checker| checker.reg(5, 0x12345000).pc(0x2002),
    );
}

#[test]
fn test_rv_c_load_store() {
    run_test_exec(
        RiscvInstr::C_LW,
        RVInstrInfo::CL {
            rd: 8,
            rs1: 9,
            imm: 8,
        },
        |builder| {
            builder
                .reg(9, ram_config::BASE_ADDR)
                .mem_base::<u32>(8, 0x8000_0001)
                .pc(0x2000)
        },
        |checker| checker.reg(8, 0xffff_ffff_8000_0001).pc(0x2002),
    );
    run_test_exec(
        RiscvInstr::C_LD,
        RVInstrInfo::CL {
            rd: 8,
            rs1: 9,
            imm: 8,
        },
        |builder| {
            builder
                .reg(9, ram_config::BASE_ADDR)
                .mem_base::<u64>(8, 0x1122_3344_5566)
                .pc(0x2000)
        },
        |checker| checker.reg(8, 0x1122_3344_5566).pc(0x2002),
    );

    run_test_exec(
        RiscvInstr::C_SW,
        RVInstrInfo::CS {
            rs1: 9,
            rs2: 8,
            imm: 8,
        },
        |builder| {
            builder
                .reg(9, ram_config::BASE_ADDR)
                .reg(8, 0xdead)
                .pc(0x2000)
        },
        |checker| checker.mem_base::<u32>(8, 0xdead).pc(0x2002),
    );
    run_test_exec(
        RiscvInstr::C_SD,
        RVInstrInfo::CS {
            rs1: 9,
            rs2: 8,
            imm: 8,
        },
        |builder| {
            builder
                .reg(9, ram_config::BASE_ADDR)
                .reg(8, 0x1122_3344_5566)
                .pc(0x2000)
        },
        |checker| checker.mem_base::<u64>(8, 0x1122_3344_5566).pc(0x2002),
    );

    run_test_exec(
        RiscvInstr::C_LWSP,
        RVInstrInfo::CI { rd_rs1: 8, imm: 8 },
        |builder| {
            builder
                .reg(2, ram_config::BASE_ADDR)
                .mem_base::<u32>(8, 0x55)
                .pc(0x2000)
        },
        |checker| checker.reg(8, 0x55).pc(0x2002),
    );
    run_test_exec(
        RiscvInstr::C_LDSP,
        RVInstrInfo::CI { rd_rs1: 8, imm: 8 },
        |builder| {
            builder
                .reg(2, ram_config::BASE_ADDR)
                .mem_base::<u64>(8, 0x99)
                .pc(0x2000)
        },
        |checker| checker.reg(8, 0x99).pc(0x2002),
    );
    run_test_exec(
        RiscvInstr::C_SWSP,
        RVInstrInfo::CSS { rs2: 8, imm: 8 },
        |builder| {
            builder
                .reg(2, ram_config::BASE_ADDR)
                .reg(8, 0xbeef)
                .pc(0x2000)
        },
        |checker| checker.mem_base::<u32>(8, 0xbeef).pc(0x2002),
    );
    run_test_exec(
        RiscvInstr::C_SDSP,
        RVInstrInfo::CSS { rs2: 8, imm: 8 },
        |builder| {
            builder
                .reg(2, ram_config::BASE_ADDR)
                .reg(8, 0xc0ff_ee00)
                .pc(0x2000)
        },
        |checker| checker.mem_base::<u64>(8, 0xc0ff_ee00).pc(0x2002),
    );
}

#[test]
fn test_rv_c_branch_jump() {
    run_test_exec(
        RiscvInstr::C_BEQZ,
        RVInstrInfo::CB {
            rd_rs1: 8,
            imm: negative_of(4),
        },
        |builder| builder.reg(8, 0).pc(0x2000),
        |checker| checker.pc(0x2000 - 4),
    );
    run_test_exec(
        RiscvInstr::C_BEQZ,
        RVInstrInfo::CB { rd_rs1: 8, imm: 16 },
        |builder| builder.reg(8, 1).pc(0x2000),
        |checker| checker.pc(0x2002),
    );
    run_test_exec(
        RiscvInstr::C_BNEZ,
        RVInstrInfo::CB { rd_rs1: 8, imm: 8 },
        |builder| builder.reg(8, 5).pc(0x2000),
        |checker| checker.pc(0x2008),
    );
    run_test_exec(
        RiscvInstr::C_BNEZ,
        RVInstrInfo::CB { rd_rs1: 8, imm: 8 },
        |builder| builder.reg(8, 0).pc(0x2000),
        |checker| checker.pc(0x2002),
    );

    run_test_exec(
        RiscvInstr::C_J,
        RVInstrInfo::CJ {
            target: negative_of(0x10),
        },
        |builder| builder.pc(0x2000),
        |checker| checker.pc(0x2000 - 0x10),
    );
    run_test_exec(
        RiscvInstr::C_JAL,
        RVInstrInfo::CJ { target: 0x20 },
        |builder| builder.pc(0x2000),
        |checker| checker.reg(1, 0x2002).pc(0x2020),
    );
    run_test_exec(
        RiscvInstr::C_JALR,
        RVInstrInfo::CR { rd_rs1: 5, rs2: 0 },
        |builder| builder.reg(5, 0x3000).pc(0x2000),
        |checker| checker.reg(1, 0x2002).pc(0x3000),
    );
    run_test_exec(
        RiscvInstr::C_JR,
        RVInstrInfo::CR { rd_rs1: 5, rs2: 0 },
        |builder| builder.reg(5, 0x3000).pc(0x2000),
        |checker| checker.pc(0x3000),
    );
}

#[test]
fn test_rv_c_float() {
    run_test_exec(
        RiscvInstr::C_FLD,
        RVInstrInfo::CL {
            rd: 8,
            rs1: 9,
            imm: 8,
        },
        |builder| {
            builder
                .reg(9, ram_config::BASE_ADDR)
                .mem_base::<u64>(8, 3.5f64.to_bits())
                .pc(0x2000)
        },
        |checker| checker.reg_f64(8, 3.5).pc(0x2002),
    );
    run_test_exec(
        RiscvInstr::C_FSD,
        RVInstrInfo::CS {
            rs1: 9,
            rs2: 8,
            imm: 8,
        },
        |builder| {
            builder
                .reg(9, ram_config::BASE_ADDR)
                .reg_f64(8, 2.5)
                .pc(0x2000)
        },
        |checker| checker.mem_base::<u64>(8, 2.5f64.to_bits()).pc(0x2002),
    );

    run_test_exec(
        RiscvInstr::C_FLDSP,
        RVInstrInfo::CI { rd_rs1: 8, imm: 8 },
        |builder| {
            builder
                .reg(2, ram_config::BASE_ADDR)
                .mem_base::<u64>(8, 1.25f64.to_bits())
                .pc(0x2000)
        },
        |checker| checker.reg_f64(8, 1.25).pc(0x2002),
    );
    run_test_exec(
        RiscvInstr::C_FSDSP,
        RVInstrInfo::CSS { rs2: 8, imm: 8 },
        |builder| {
            builder
                .reg(2, ram_config::BASE_ADDR)
                .reg_f64(8, 6.0)
                .pc(0x2000)
        },
        |checker| checker.mem_base::<u64>(8, 6.0f64.to_bits()).pc(0x2002),
    );
}

#[cfg(feature = "custom-instr")]
#[ignore = "custom-instr"]
#[test]
fn test_custom_instr() {
    run_test_cpu_step(
        &[
            0b00001011000_00000_001_00000_0101011,
            0b00000001010_00000_001_00000_0101011,
        ],
        |builder| builder,
        |checker| checker,
    );
}

#[test]
fn test_default_csr_value() {
    let cpu = TestCPUBuilder::new().build();

    #[cfg(feature = "riscv32")]
    assert_eq!(
        cpu.csr
            .read_uncheck_privilege(csr_index::mstatus)
            .unwrap()
            .extract_bits(32, 33),
        1
    );

    #[cfg(feature = "riscv64")]
    assert_eq!(
        cpu.csr
            .read_uncheck_privilege(csr_index::mstatus)
            .unwrap()
            .extract_range(32, 33),
        2
    );
}

#[test]
fn test_amo() {
    use std::sync::atomic::{AtomicU64, Ordering};

    const CNT: usize = 4096;
    const TARGET_ADDR: WordType = ram_config::BASE_ADDR + 1024;
    let mut cpu = TestCPUBuilder::new()
        .reg(12, TARGET_ADDR)
        .reg(11, 1)
        .program(&[0x00b6302f, 0xffdff06f]) // label: amoadd.d x0, a1, (a2); j label
        .build();

    let ptr = cpu.memory.get_raw_ptr();
    let atomic_ptr_addr =
        unsafe { ptr.add((TARGET_ADDR - ram_config::BASE_ADDR) as usize) as *const AtomicU64 }
            as usize;

    thread::scope(|scope| {
        scope.spawn(move || {
            let ptr = atomic_ptr_addr as *const AtomicU64;
            for _ in 0..CNT {
                unsafe {
                    (*ptr).fetch_add(1, Ordering::Relaxed);
                    print!("A");
                }
            }
        });

        // The target address increases every 2 steps.
        for _ in 0..CNT {
            cpu.step().unwrap();
            cpu.step().unwrap();
            print!("B");
        }
    });

    let val: u64 = cpu.memory.read_by_paddr(TARGET_ADDR).unwrap();
    assert_eq!(val, (CNT * 2) as u64);
}

#[test]
fn test_vector_config() {
    run_test_exec(
        RiscvInstr::VSETVL,
        RVInstrInfo::V {
            rs1: 1,
            rs2: 2,
            rd: 3,
            vm: false,
            func6: 0b100000,
        },
        |builder| builder.reg(1, 100).reg(2, 0b00_010_001).pc(0x2000), // sew = 32, lmul = 2
        |checker| {
            checker
                .reg(3, VLEN as WordType * 2 / 32)
                .csr(Vtype::get_index(), 0b00_010_001)
                .pc(0x2004)
        },
    );

    run_test_exec(
        RiscvInstr::VSETVLI,
        RVInstrInfo::V {
            rs1: 1,
            rs2: 0b10_001,
            rd: 3,
            vm: false,
            func6: 0b000000,
        },
        |builder| builder.reg(1, 100).reg(2, 0).pc(0x2000), // sew = 32, lmul = 2
        |checker| {
            checker
                .reg(3, VLEN as WordType * 2 / 32)
                .csr(Vtype::get_index(), 0b00_010_001)
                .pc(0x2004)
        },
    );

    run_test_exec(
        RiscvInstr::VSETIVLI,
        RVInstrInfo::V {
            rs1: 0b11111,
            rs2: 0b11_001,
            rd: 3,
            vm: false,
            func6: 0b110000,
        },
        |builder| builder.pc(0x2000), // sew = 32, lmul = 2
        |checker| {
            checker
                .reg(3, VLEN as WordType * 2 / 64)
                .csr(Vtype::get_index(), 0b00_011_001)
                .pc(0x2004)
        },
    );

    run_test_exec_decode(
        0b1000000_00010_00001_111_00011_1010111, // vsetvl x3, x1, x2
        |builder| builder.reg(1, 100).reg(2, 0b00_010_001).pc(0x2000),
        |checker| {
            checker
                .reg(3, VLEN as WordType * 2 / 32)
                .csr(Vtype::get_index(), 0b00_010_001)
                .pc(0x2004)
        },
    );

    run_test_exec_decode(
        0b000000_010_001_00001_111_00011_1010111, // vsetvli x3, x1, e32, m2, ta, ma
        |builder| builder.reg(1, 100).pc(0x2000),
        |checker| {
            checker
                .reg(3, VLEN as WordType * 2 / 32)
                .csr(Vtype::get_index(), 0b00_010_001)
                .pc(0x2004)
        },
    );

    run_test_exec_decode(
        0b110000_010_001_10000_111_00011_1010111, // vsetivli x3, 32, e32, m2, ta, ma
        |builder| builder.pc(0x2000),
        |checker| {
            checker
                .reg(3, VLEN as WordType * 2 / 32)
                .csr(Vtype::get_index(), 0b00_010_001)
                .pc(0x2004)
        },
    );

    // maxvl > vl
    run_test_exec_decode(
        0b110000_010_001_00010_111_00011_1010111, // vsetivli x3, 32, e32, m2, ta, ma
        |builder| builder.pc(0x2000),
        |checker| {
            checker
                .reg(3, 2)
                .csr(Vtype::get_index(), 0b00_010_001)
                .pc(0x2004)
        },
    );
}
