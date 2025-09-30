#[cfg(not(test))]
use crossterm::terminal::disable_raw_mode;

use crate::{
    board::virt::IRQHandler,
    config::arch_config::{REG_NAME, REGFILE_CNT, WordType},
    cpu::RegFile,
    fpu::soft_float::SoftFPU,
    isa::{
        DecoderTrait,
        icache::{ICache, SetICache},
        riscv::{
            RiscvTypes,
            csr_reg::{CsrRegFile, PrivilegeLevel, csr_macro::*},
            decoder::{DecodeInstr, Decoder},
            instruction::{RVInstrInfo, exec_mapping::get_exec_func, rv32i_table::RiscvInstr},
            mmu::VirtAddrManager,
            trap::{Exception, Interrupt, Trap, trap_controller::TrapController},
        },
    },
    ram_config::DEFAULT_PC_VALUE,
};

pub struct RV32CPU {
    pub(super) reg_file: RegFile,
    pub(super) memory: VirtAddrManager,
    pub(super) pc: WordType,
    pub(super) decoder: Decoder,
    pub(super) csr: CsrRegFile,
    pub(super) icache: SetICache<RiscvTypes, 64, 8>,
    pub(super) fpu: SoftFPU,
    pub icache_cnt: usize,

    /// The trap value pending to be written to `mtval`/`stval`.
    pub(super) pending_tval: Option<WordType>,
}

impl RV32CPU {
    pub fn from_vaddr_manager(v_memory: VirtAddrManager) -> Self {
        let mut csr = CsrRegFile::new();

        // TODO: Record extensions in Decoder.
        let ext = "FIMSU"
            .chars()
            .into_iter()
            .map(|c| c as WordType - 'A' as WordType)
            .fold(0, |acc, c| acc | (1 << c));
        csr.ctx.extension = ext;

        let mxl = if WordType::BITS == 32 {
            1
        } else {
            debug_assert!(WordType::BITS == 64);
            2
        };

        csr.get_by_type::<Misa>()
            .unwrap()
            .set_extension_directly(ext);
        csr.get_by_type::<Misa>().unwrap().set_mxl_directly(mxl);
        csr.get_by_type::<Mstatus>().unwrap().set_sxl_directly(mxl);
        csr.get_by_type::<Mstatus>().unwrap().set_uxl_directly(mxl);
        csr.get_by_type::<Sstatus>().unwrap().set_uxl_directly(mxl);

        debug_assert!(csr.get_by_type::<Mstatus>().unwrap().get_uxl() == mxl);

        csr.set_current_privileged(PrivilegeLevel::M);

        Self {
            reg_file: RegFile::new(),
            memory: v_memory,
            pc: DEFAULT_PC_VALUE,
            decoder: Decoder::new(),
            csr: csr,
            icache: SetICache::new(),
            fpu: SoftFPU::new(),
            icache_cnt: 0,
            pending_tval: None,
        }
    }

    pub(in super::super) fn execute(
        &mut self,
        instr: RiscvInstr,
        info: RVInstrInfo,
    ) -> Result<(), Exception> {
        let rst = get_exec_func(instr)(info, self);
        self.reg_file[0] = 0;

        rst
    }

    // TODO: Move or delete this when the debugger is implemented
    fn debug_reg_string(&self) -> String {
        let mut s = String::new();
        for i in 0..REGFILE_CNT {
            if self.reg_file[i] == 0 {
                continue;
            }

            s.push_str(&format!("{}: 0x{:x}", REG_NAME[i], self.reg_file[i]));
            if i != REGFILE_CNT - 1 {
                s.push_str(", ");
            }
        }
        s
    }

    pub fn step(&mut self) -> Result<(), Exception> {
        if let Some(interrupt) = TrapController::check_interrupt(self) {
            TrapController::send_trap_signal(self, Trap::Interrupt(interrupt), 0);
            return Ok(());
        }

        let DecodeInstr(instr, info) = if let Some(decode_instr) = self.icache.get(self.pc) {
            self.icache_cnt += 1;
            decode_instr
        } else {
            // IF
            let instr_bytes = self.memory.read::<u32>(self.pc);
            if let Err(err) = instr_bytes {
                TrapController::send_trap_signal(
                    self,
                    Trap::Exception(Exception::from_instr_fetch_err(err)),
                    0,
                );
                return Ok(());
            }
            let instr_bytes = unsafe { instr_bytes.unwrap_unchecked() };
            log::trace!(
                "I-Cache not hit, raw instruction: {:#x} at {:#x}",
                instr_bytes,
                self.pc
            );

            // ID
            let decoder_result = self.decoder.decode(instr_bytes);
            if let None = decoder_result {
                log::warn!("Illegal instruction: {:#x} at {:#x}", instr_bytes, self.pc);
                TrapController::send_trap_signal(
                    self,
                    Trap::Exception(Exception::IllegalInstruction),
                    instr_bytes as WordType,
                );
                return Ok(());
            }

            let decode_instr = unsafe { decoder_result.unwrap_unchecked() };
            self.icache.put(self.pc, decode_instr.clone());
            decode_instr
        };

        log::trace!("Decoded instruction: {:#?}, info: {:?}", instr, info);

        // EX && MEM && WB
        let excute_result = self.execute(instr, info);
        match excute_result {
            Err(Exception::Breakpoint) => return excute_result,
            Err(nr) => {
                TrapController::send_trap_signal(self, Trap::Exception(nr), 0);
                return Ok(());
            }
            Ok(()) => {} // there is nothing to do.
        }

        return Ok(());
    }

    pub fn clear_all_cache(&mut self) {
        self.icache.clear();
    }

    pub fn power_off(&mut self) -> Result<(), Exception> {
        self.memory.sync();
        #[cfg(not(test))]
        disable_raw_mode().unwrap();
        Ok(())
    }
}

impl IRQHandler for RV32CPU {
    fn handle_irq(&mut self, id: u8, level: bool) {
        match Interrupt::from(id as usize) {
            Interrupt::MachineTimer => {
                if level {
                    self.csr.get_by_type::<Mip>().unwrap().set_mtip(1);
                } else {
                    self.csr.get_by_type::<Mip>().unwrap().set_mtip(0);
                }
            }

            _ => {
                todo!("IRQ handling not implemented yet.")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::f32;

    use super::*;
    use crate::{
        isa::riscv::{cpu_tester::*, csr_reg::csr_index},
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
            0xfcf42e23, //          	sw	a5,-36(s0)
            |builder| builder.reg(15, 123).reg(8, ram_config::BASE_ADDR + 72),
            |checker| checker.mem_base::<u32>(36, 123),
        );
    }

    #[test]
    fn test_u_types_decode() {
        // TODO: Test signed extend of `auipc`
        run_test_exec_decode(
            0x12233097, // auipc x1, 0x112233
            |builder| builder.reg(1, 3).pc(0x1000),
            |checker| checker.reg(1, 0x12234000).pc(0x1004),
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

        run_test_exec_decode(
            0xf8c318e3, // bne x6, x12, -112
            |builder| builder.reg(6, 5).reg(12, 5).pc(0x2000),
            |checker| checker.pc(0x2004),
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
        // TODO: This test is disabled because some bits in mstatus are `WPRI`,
        // and these bits should always be 0.
        // Choose another CSR or number.

        // 1) CSRRW x11, mstatus(0x300), x5
        // run_test_exec_decode(
        //     0x300295f3,
        //     |builder| builder.reg(5, 0xAAAA).csr(0x300, 0x1234).pc(0x1000),
        //     |checker| checker.reg(11, 0x1234).csr(0x300, 0xAAAA).pc(0x1004),
        // );

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
    fn test_rv_f() {
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
                .extract_bits(32, 33),
            2
        );
    }
}
