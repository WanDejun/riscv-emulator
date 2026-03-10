use std::hint::cold_path;

#[cfg(not(test))]
use crossterm::terminal::disable_raw_mode;

use crate::{
    board::virt::RiscvIRQHandler,
    config::arch_config::WordType,
    cpu::RegFile,
    device::MemError,
    fpu::soft_float::SoftFPU,
    isa::{
        DecoderTrait,
        cache::{Cache, SetCache},
        riscv::{
            csr_reg::{CsrRegFile, NamedCsrReg, PrivilegeLevel, csr_macro::*},
            decoder::{DecodeInstr, Decoder},
            instruction::{RVInstrInfo, exec_mapping::get_exec_func, instr_table::RiscvInstr},
            mmu::VirtAddrManager,
            trap::{Exception, Interrupt, Trap, trap_controller::TrapController},
        },
    },
    ram_config::DEFAULT_PC_VALUE,
    utils::make_mask,
};

#[derive(Clone)]
pub struct ExcuteInstrInfo {
    pub instr: Option<DecodeInstr>,
    pub trap: bool,
}

impl ExcuteInstrInfo {
    pub fn new() -> Self {
        Self {
            instr: None,
            trap: false,
        }
    }
}

pub(crate) struct DebugInfo {
    pub(crate) last_instr: ExcuteInstrInfo,
}

impl DebugInfo {
    pub fn new() -> Self {
        Self {
            last_instr: ExcuteInstrInfo::new(),
        }
    }
}

pub struct RVCPU {
    pub(crate) debug: bool,
    pub(crate) debug_info: DebugInfo,

    pub(crate) icache_cnt: usize,

    pub(super) reg_file: RegFile,
    pub(super) memory: VirtAddrManager,
    pub(super) pc: WordType,
    pub(super) decoder: Decoder,
    pub(super) csr: CsrRegFile,
    pub(super) icache: SetCache<DecodeInstr, 256, 8>,
    pub(super) fpu: SoftFPU,

    /// The address of the memory-mapped `mtime` CSR.
    pub(crate) time_addr: Option<WordType>,

    /// The trap value pending to be written to `mtval`/`stval`.
    pub(super) pending_tval: Option<WordType>,
}

impl RVCPU {
    pub(crate) fn from_vaddr_manager(v_memory: VirtAddrManager) -> Self {
        let mut csr = CsrRegFile::new();

        // TODO: Record extensions in Decoder.
        let ext = "ADFIMSU"
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

        csr.get_by_type_existing::<Misa>()
            .set_extension_directly(ext);
        csr.get_by_type_existing::<Misa>().set_mxl_directly(mxl);
        csr.get_by_type_existing::<Mstatus>().set_sxl_directly(mxl);
        csr.get_by_type_existing::<Mstatus>().set_uxl_directly(mxl);

        debug_assert!(csr.get_by_type_existing::<Mstatus>().get_uxl() == mxl);
        debug_assert!(csr.get_by_type_existing::<Mstatus>().get_sxl() == mxl);
        debug_assert!(csr.get_by_type_existing::<Sstatus>().get_uxl() == mxl);

        csr.set_current_privileged(PrivilegeLevel::M);

        let fpu = SoftFPU::from(true);

        Self {
            debug: false,
            debug_info: DebugInfo::new(),
            icache_cnt: 0,
            reg_file: RegFile::new(),
            memory: v_memory,
            pc: DEFAULT_PC_VALUE,
            decoder: Decoder::new(),
            csr: csr,
            icache: SetCache::new(),
            fpu,
            time_addr: None,
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

        if let Err(ex) = rst {
            cold_path();

            // Avoid logging common/normal exceptions.
            const IGNORE_EXCEPTIONS: &[Exception] = &[
                Exception::LoadMisaligned, // We need OpenSBI to support misaligned access
                Exception::StoreMisaligned,
                Exception::UserEnvCall,
                Exception::SupervisorEnvCall,
                Exception::MachineEnvCall,
            ];

            if IGNORE_EXCEPTIONS.contains(&ex) == false {
                cold_path();

                if ex == Exception::IllegalInstruction {
                    log::warn!(
                        "IllegalInstruction for instr: {:#?} at pc = {:#x}, info: {:?} ",
                        instr,
                        self.pc,
                        info,
                    );
                } else {
                    log::info!(
                        "Exception {:?} for instr: {:#?} at pc = {:#x}, xtval = {:#x}, info: {:?}",
                        ex,
                        instr,
                        self.pc,
                        self.pending_tval.unwrap_or(0),
                        info
                    );
                }
            }
        }

        rst
    }

    pub fn read_csr(&mut self, addr: WordType) -> Result<WordType, Exception> {
        if addr == 0xc01 {
            // time CSR
            if let Some(time_addr) = self.time_addr {
                if let Ok(time) = self.memory.read_by_paddr::<u64>(time_addr) {
                    return Ok(time as WordType);
                }
            }
        } else if let Some(data) = self.csr.read(addr) {
            // Normal CSR read
            return Ok(data);
        }

        Err(Exception::IllegalInstruction)
    }

    /// Write CSR and update context correctly.
    ///
    /// XXX: Use this function instead of `self.csr.write`, unless you are sure about what you are doing.
    ///
    /// You may need [`CsrRegFile::write_directly`] in some cases.
    pub fn write_csr(&mut self, addr: WordType, data: WordType) -> Result<(), Exception> {
        if let None = self.csr.write(addr, data) {
            log::warn!("Failed to write CSR {:#x} with data {:#x}", addr, data);
            return Err(Exception::IllegalInstruction);
        }

        if addr == Satp::get_index() {
            let satp = self.csr.get_by_type_existing::<Satp>();
            self.memory.set_mode(satp.get_mode() as u8);
            self.memory.set_root_ppn(satp.get_ppn() as u64);
        }

        Ok(())
    }

    pub fn step(&mut self) -> Result<(), Exception> {
        if self.debug {
            self.debug_info.last_instr.trap = false;
        }

        let rst = self.step_impl();

        let mcycle = self.csr.get_by_type_existing::<Mcycle>();
        mcycle.set_mcycle_directly(mcycle.data().wrapping_add(1));

        debug_assert!(self.pending_tval.is_none());

        rst
    }

    fn ifetch(&mut self) -> Result<u32, MemError> {
        let mut instr_bytes: u32 = self.memory.ifetch::<u16>(self.pc, &mut self.csr)? as u32;

        if (instr_bytes & 0b11) == 0b11 {
            // 32-bit instr
            let next_half = self.memory.ifetch::<u16>(self.pc + 2, &mut self.csr)? as u32;
            instr_bytes |= next_half << 16;
        };

        Ok(instr_bytes)
    }

    fn step_impl(&mut self) -> Result<(), Exception> {
        if let Some(interrupt) = TrapController::has_interrupt(self) {
            if TrapController::try_send_trap_signal(self, Trap::Interrupt(interrupt), 0) {
                return Ok(());
            }
        }

        let DecodeInstr(instr, info) = if let Some(decode_instr) = self.icache.get(self.pc) {
            self.icache_cnt += 1;
            decode_instr
        } else {
            let instr_bytes = match self.ifetch() {
                Ok(bytes) => bytes,
                Err(err) => {
                    TrapController::try_send_trap_signal(
                        self,
                        Trap::Exception(Exception::from_instr_fetch_err(err)),
                        self.pc,
                    );
                    return Ok(());
                }
            };

            // ID

            // TODO: We have to support C.nop for riscv-arch-test,
            // while currently we don't support the C extension.
            // So a temparary workaround is added here.
            if (instr_bytes & (make_mask(13, 15) | make_mask(7, 11) | 0b11) as u32) == 0x0001 {
                self.pc = self.pc.wrapping_add(2);
                return Ok(());
            }

            let decoder_result = self.decoder.decode(instr_bytes);
            if let None = decoder_result {
                log::warn!("Illegal instruction: {:#x} at {:#x}", instr_bytes, self.pc);
                TrapController::try_send_trap_signal(
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

        if self.debug {
            self.debug_info.last_instr = ExcuteInstrInfo {
                instr: Some(DecodeInstr(instr, info.clone())),
                trap: false,
            };
        }

        // EX && MEM && WB
        let excute_result = self.execute(instr, info);
        match excute_result {
            // XXX: OpenSBI have semihosting test, and we don't implement breakpoint exception handling yet,
            // so we can't throw and panic here.
            // Err(Exception::Breakpoint) => return excute_result,
            Err(Exception::IllegalInstruction) => {
                // TODO: Consider reuse the fetched instr_bytes
                // (do we have to put raw instruction in i-cache to avoid another ifetch here?)
                let instr_bytes = self.ifetch().expect("ifetch should not fail here");
                TrapController::try_send_trap_signal(
                    self,
                    Trap::Exception(Exception::IllegalInstruction),
                    instr_bytes as WordType,
                );
            }
            Err(nr) => {
                TrapController::try_send_trap_signal(self, Trap::Exception(nr), 0);
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

impl RiscvIRQHandler for RVCPU {
    fn handle_irq(&mut self, interrupt: Interrupt, level: bool) {
        log::trace!("Handling IRQ: {:?}, level: {}", interrupt, level);
        let mip = self.csr.get_by_type_existing::<Mip>();
        let level = level as WordType;

        match interrupt {
            Interrupt::MachineTimer => {
                mip.set_mtip(level);
            }
            Interrupt::MachineExternal => {
                mip.set_meip(level);
            }
            Interrupt::SupervisorExternal => {
                mip.set_seip(level);
            }

            Interrupt::MachineSoft => {
                mip.set_msip(level);
            }

            _ => {
                todo!("IRQ handling not implemented yet.")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{f32, thread};

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
            0xfcf42e23, // sw a5,-36(s0)
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
}
