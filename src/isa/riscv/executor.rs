#[cfg(not(test))]
use crossterm::terminal::disable_raw_mode;

use crate::{
    config::arch_config::{REG_NAME, REGFILE_CNT, WordType},
    cpu::RegFile,
    device::{DeviceTrait, Mem},
    isa::riscv::{
        csr_reg::CsrRegFile,
        decoder::{DecodeInstr, Decoder},
        instruction::{RVInstrInfo, exec_mapping::get_exec_func, rv32i_table::RiscvInstr},
        trap::{Exception, Trap, trap_controller::TrapController},
        vaddr::VirtAddrManager,
    },
    ram_config::DEFAULT_PC_VALUE,
};

pub struct RV32CPU {
    pub(super) reg_file: RegFile,
    pub(super) memory: VirtAddrManager,
    pub(super) pc: WordType,
    pub(super) decoder: Decoder,
    pub(super) csr: CsrRegFile,
}

impl RV32CPU {
    pub fn new() -> Self {
        Self::from_memory(VirtAddrManager::new())
    }

    // TODO: A builder struct may be useful for future use.
    pub fn from_memory(v_memory: VirtAddrManager) -> Self {
        Self {
            reg_file: RegFile::new(),
            memory: v_memory,
            pc: DEFAULT_PC_VALUE,
            decoder: Decoder::new(),
            csr: CsrRegFile::new(),
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
        // IF
        let instr_bytes = self.memory.read::<u32>(self.pc);
        if let Err(err) = instr_bytes {
            TrapController::send_trap_signal(
                self,
                Trap::Exception(Exception::from_instr_fetch_err(err)),
                self.pc,
                self.pc,
            );
            return Ok(());
        }
        let instr_bytes = unsafe { instr_bytes.unwrap_unchecked() };
        log::trace!("raw instruction: {:#x} at pc {:#x}", instr_bytes, self.pc);

        // ID
        let decoder_result = self.decoder.decode(instr_bytes);
        if let Err(nr) = decoder_result {
            TrapController::send_trap_signal(self, Trap::Exception(nr), self.pc, self.pc);
            return Ok(());
        }
        let DecodeInstr(instr, info) = unsafe { decoder_result.unwrap_unchecked() };
        log::trace!("Decoded instruction: {:#?}, info: {:?}", instr, info);

        // EX && MEM && WB
        let excute_result = self.execute(instr, info);
        match excute_result {
            Err(Exception::Breakpoint) => return excute_result,
            Err(nr) => {
                TrapController::send_trap_signal(self, Trap::Exception(nr), self.pc, self.pc);
                return Ok(());
            }
            Ok(()) => {} //there is nothing todo.
        }

        self.memory.step();
        log::trace!("{}", self.debug_reg_string());
        return Ok(());
    }

    pub fn power_off(&mut self) -> Result<(), Exception> {
        self.memory.sync();
        #[cfg(not(test))]
        disable_raw_mode().unwrap();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        isa::riscv::{
            cpu_tester::*,
            csr_reg::{csr_index, csr_macro::Mcause},
        },
        ram_config,
        utils::{negative_of, sign_extend},
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

        for _i in 1..=100 {
            tester.test_rand_r(RiscvInstr::ADD, |lhs, rhs| lhs.wrapping_add(rhs));
            tester.test_rand_r(RiscvInstr::SUB, |lhs, rhs| lhs.wrapping_sub(rhs));
            tester.test_rand_i(RiscvInstr::ADDI, |lhs, imm| lhs.wrapping_add(imm));

            // TODO: Add some handmade data,
            // because tests and actual codes are actually written in similar ways.
            tester.test_rand_i(RiscvInstr::SLTI, |lhs, imm| {
                ((lhs.cast_signed()) < (sign_extend(imm, 12).cast_signed())) as WordType
            });
            tester.test_rand_i(RiscvInstr::SLTIU, |lhs, imm| {
                ((lhs) < (sign_extend(imm, 12))) as WordType
            });
        }
    }

    #[test]
    fn test_exec_arith_decode() {
        // TODO: add checks for boundary cases
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
        // 1) CSRRW x11, mstatus(0x300), x5
        run_test_exec_decode(
            0x300295f3,
            |builder| builder.reg(5, 0xAAAA).csr(0x300, 0x1234),
            |checker| checker.reg(11, 0x1234).csr(0x300, 0xAAAA),
        );

        // 2) CSRRS x12, mtvec(0x305), x6
        run_test_exec_decode(
            0x30532673,
            |builder| builder.reg(6, 0x00F0).csr(0x305, 0x0F00),
            |checker| checker.reg(12, 0x0F00).csr(0x305, 0x0FF0),
        );

        // 3) CSRRC x13, mepc(0x341), x7
        run_test_exec_decode(
            0x3413b6f3,
            |builder| builder.reg(7, 0x0FF0).csr(0x341, 0x0FFF),
            |checker| checker.reg(13, 0x0FFF).csr(0x341, 0x000F),
        );

        // 4) CSRRWI x11, mcause(0x342), imm=5
        run_test_exec_decode(
            0x3422d5f3,
            |builder| builder.csr(0x342, 0xABCD),
            |checker| checker.reg(11, 0xABCD).csr(0x342, 5),
        );

        // 5) CSRRSI x12, mip(0x344), imm=6
        run_test_exec_decode(
            0x34436673,
            |builder| builder.csr(0x344, 0x00F0),
            |checker| checker.reg(12, 0x00F0).csr(0x344, 0x00F6),
        );

        // 6) CSRRCI x13, mie(0x304), imm=7
        run_test_exec_decode(
            0x3043f6f3,
            |builder| builder.csr(0x304, 0x00FF),
            |checker| checker.reg(13, 0x00FF).csr(0x304, 0x00F8),
        );
    }

    #[test]
    fn test_illgal_instr() {
        run_test_cpu_step(
            &[0x00003503], // ld a0, 0(zero)
            |builder| builder.csr(csr_index::mtvec, 0x00FF << 2),
            |checker| {
                checker
                    .pc(0x00FF << 2)
                    .csr(csr_index::mepc, ram_config::BASE_ADDR)
                    .customized(|checker| {
                        let mcause = checker.cpu.csr.get_by_type::<Mcause>();
                        assert_eq!(mcause.get_interrupt(), 0);
                        assert_eq!(mcause.get_exception_code(), Exception::LoadFault.into());
                        checker
                    })
            },
        );
    }
}
