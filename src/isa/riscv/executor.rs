use std::hint::cold_path;

use crate::{
    board::virt::RiscvIRQHandler,
    config::arch_config::WordType,
    cpu::RegFile,
    device::MemError,
    fpu::soft_float::SoftFPU,
    isa::{
        InstrLen,
        cache::{Cache, SetCache},
        riscv::{
            RawInstr,
            csr_reg::{CsrRegFile, NamedCsrReg, PrivilegeLevel, csr_macro::*},
            decoder::{DecodeInstr, Decoder},
            instruction::{RVInstrInfo, exec_mapping::get_exec_func, instr_table::RiscvInstr},
            mmu::VirtAddrManager,
            trap::{Exception, Interrupt, Trap, trap_controller::TrapController},
            vector::Vector,
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
    pub(super) vector: Vector,

    /// The address of the memory-mapped `mtime` CSR.
    pub(crate) time_addr: Option<WordType>,

    /// The trap value pending to be written to `mtval`/`stval`.
    pub(super) pending_tval: Option<WordType>,
}

impl RVCPU {
    pub(crate) fn from_vaddr_manager(v_memory: VirtAddrManager) -> Self {
        Self::from_decoder(Decoder::new(), v_memory)
    }

    pub(crate) fn from_decoder(decoder: Decoder, v_memory: VirtAddrManager) -> Self {
        let mut csr = CsrRegFile::new();

        let ext = decoder.extension_bits();
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
            decoder,
            csr: csr,
            vector: Vector::new(),
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
        // Replacing function-pointer dispatch in `get_exec_func` with immediate call to the execution function,
        // makes the program 10%-20% slower on my machine.
        // This is likely because it hurts jump-table dispatch and pulls some cold paths into the hot path.
        let rst = get_exec_func(instr)(info, self);
        self.reg_file[0] = 0;

        if let Err(ex) = rst {
            cold_path();

            if ex == Exception::IllegalInstruction {
                log::warn!(
                    "IllegalInstruction for instr: {:#?} at pc = {:#x}, info: {:?} ",
                    instr,
                    self.pc,
                    info,
                );
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
        if !self.csr.write(addr, data) {
            log::warn!("Failed to write CSR {:#x} with data {:#x}", addr, data);
            return Err(Exception::IllegalInstruction);
        }

        // Changing satp.MODE from Bare to other modes and vice versa also takes effect immediately,
        // without the need to execute an SFENCE.VMA instruction.
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

    fn ifetch(&mut self) -> Result<RawInstr, MemError> {
        let mut bytes: RawInstr =
            (self.memory.ifetch::<u16>(self.pc, &mut self.csr)? as u32).into();

        if bytes.len() == 4 {
            // 32-bit instr.

            // "The C extension allows 16-bit instructions to be freely intermixed with 32-bit instructions,
            // with the latter now able to start on any 16-bit boundary."

            // but the next half may sit on the next page, causing a page fault.
            let next_half = match self.memory.ifetch::<u16>(self.pc + 2, &mut self.csr) {
                Ok(half) => half as u32,
                Err(err) => {
                    self.pending_tval = Some(self.pc + 2);
                    return Err(err);
                }
            };
            bytes.val |= next_half << 16;
        };

        Ok(bytes)
    }

    fn step_impl(&mut self) -> Result<(), Exception> {
        if let Some(interrupt) = TrapController::has_interrupt(self) {
            if TrapController::try_send_trap_signal(self, Trap::Interrupt(interrupt), 0) {
                return Ok(());
            }
        }

        let DecodeInstr { instr, info, len } = if let Some(decode_instr) = self.icache.get(self.pc)
        {
            self.icache_cnt += 1;
            decode_instr
        } else {
            let raw_instr = match self.ifetch() {
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
            if (raw_instr.val & (make_mask(13, 15) | make_mask(7, 11) | 0b11) as u32) == 0x0001 {
                self.pc = self.pc.wrapping_add(2);
                return Ok(());
            }

            let decoder_result = self.decoder.decode(raw_instr);
            let Some(decode_instr) = decoder_result else {
                log::warn!(
                    "Illegal instruction: {:#x} at {:#x}",
                    raw_instr.val,
                    self.pc
                );
                TrapController::try_send_trap_signal(
                    self,
                    Trap::Exception(Exception::IllegalInstruction),
                    raw_instr.val as WordType,
                );
                return Ok(());
            };

            self.icache.put(self.pc, decode_instr.clone());
            decode_instr
        };

        if self.debug {
            self.debug_info.last_instr = ExcuteInstrInfo {
                instr: Some(DecodeInstr {
                    instr,
                    info: info.clone(),
                    len,
                }),
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
                cold_path();

                // We cannot reuse the fetched raw instruction on the i-cache hit path,
                // because the raw instruction bytes are not stored in the i-cache.
                // This is acceptable because `illegal instruction` is a cold path.
                let raw_instr = self.ifetch().expect("ifetch should not fail here");
                TrapController::try_send_trap_signal(
                    self,
                    Trap::Exception(Exception::IllegalInstruction),
                    raw_instr.val as WordType,
                );
            }
            Err(nr) => {
                TrapController::try_send_trap_signal(self, Trap::Exception(nr), 0);
            }
            Ok(()) => {} // there is nothing to do.
        }

        return Ok(());
    }

    pub fn flush_icache(&mut self) {
        self.icache.clear();
    }

    pub fn flush_tlb(&mut self) {
        self.memory.flush_tlb();
    }

    pub fn power_off(&mut self) -> Result<(), Exception> {
        self.memory.sync();
        Ok(())
    }
}

impl RiscvIRQHandler for RVCPU {
    fn handle_irq(&mut self, interrupt: Interrupt, level: bool) {
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
#[path = "cpu_test.rs"]
mod test;
