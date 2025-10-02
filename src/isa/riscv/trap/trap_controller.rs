use crate::{
    config::arch_config::WordType,
    isa::{
        DebugTarget,
        riscv::{
            csr_reg::{NamedCsrReg, PrivilegeLevel, csr_index, csr_macro::*},
            executor::RV32CPU,
            trap::{Exception, Interrupt, Trap},
        },
    },
};

// TODO: Remove static class `TrapController` with sepreate modules for each privilege level.
pub(in crate::isa::riscv) struct TrapController {}

impl TrapController {
    // pub fn new() -> Self {
    //     Self {}
    // }

    // ======================================
    //                M-Mode
    // ======================================
    fn is_exception_delegated_m_mode(cpu: &mut RV32CPU, exception: Exception) -> bool {
        let medeleg = cpu.csr.get_by_type::<Medeleg>().unwrap();
        let medeleg_val = medeleg.get_medeleg();
        (medeleg_val & (1 << exception as u8)) != 0
    }

    fn is_interrupt_delegated_m_mode(cpu: &mut RV32CPU, interrupt: Interrupt) -> bool {
        let mideleg = cpu.csr.get_by_type::<Mideleg>().unwrap();
        match interrupt {
            Interrupt::MachineExternal | Interrupt::MachineSoft | Interrupt::MachineTimer => false,
            Interrupt::SupervisorExternal | Interrupt::UserExternal => mideleg.get_seip() != 0,
            Interrupt::SupervisorSoft | Interrupt::UserSoft => mideleg.get_ssip() != 0,
            Interrupt::SupervisorTimer | Interrupt::UserTimer => mideleg.get_stip() != 0,
            Interrupt::Unknown => {
                unreachable!()
            }
        }
    }

    fn is_delegated_m_mode(cpu: &mut RV32CPU, cause: Trap) -> bool {
        match cause {
            Trap::Interrupt(interrupt) => Self::is_interrupt_delegated_m_mode(cpu, interrupt),
            Trap::Exception(exception) => Self::is_exception_delegated_m_mode(cpu, exception),
        }
    }

    fn send_trap_signal_m_mode(cpu: &mut RV32CPU, cause: Trap, trap_value: WordType) {
        cpu.csr
            .get_by_type::<Mstatus>()
            .unwrap()
            .set_mpp(cpu.csr.privelege_level() as u8 as WordType);
        cpu.csr.set_current_privileged(PrivilegeLevel::M);
        cpu.csr
            .write_uncheck_privilege(Mcause::get_index(), cause.into());
        cpu.csr.write_uncheck_privilege(Mepc::get_index(), cpu.pc);

        let tval = cpu.pending_tval.take().unwrap_or(trap_value);
        cpu.csr.write_uncheck_privilege(csr_index::mtval, tval);

        let mstatus = cpu.csr.get_by_type_existing::<Mstatus>();
        mstatus.set_mpie(mstatus.get_mie());
        mstatus.set_mie(0);

        let mtvec = cpu.csr.get_by_type_existing::<Mtvec>();
        cpu.write_pc(Self::next_pc_by_tvec(
            cause,
            mtvec.get_mode(),
            mtvec.get_base(),
        ));
    }

    pub fn mret(cpu: &mut RV32CPU) {
        let mstatus = cpu.csr.get_by_type::<Mstatus>().unwrap();
        mstatus.set_mie(mstatus.get_mpie());
        mstatus.set_mpie(1);
        cpu.write_pc(cpu.csr.read_uncheck_privilege(csr_index::mepc).unwrap());

        if mstatus.get_mpp() != 3 {
            // MPP is not M-Mode, clear mprv.
            mstatus.set_mprv(0);
        }
        cpu.csr
            .set_current_privileged((mstatus.get_mpp() as u8).into());
        mstatus.set_mpp(0);
    }

    // ======================================
    //                S-Mode
    // ======================================
    fn send_trap_signal_s_mode(cpu: &mut RV32CPU, cause: Trap, trap_value: WordType) {
        cpu.csr
            .get_by_type_existing::<Sstatus>()
            .set_spp(cpu.csr.privelege_level() as u8 as WordType);
        cpu.csr.set_current_privileged(PrivilegeLevel::S);

        cpu.csr
            .write_directly(Scause::get_index(), cause.into())
            .unwrap();
        cpu.csr.write_directly(Sepc::get_index(), cpu.pc).unwrap();

        let tval = cpu.pending_tval.take().unwrap_or(trap_value);
        cpu.csr.write_directly(Stval::get_index(), tval).unwrap();

        let sstatus = cpu.csr.get_by_type_existing::<Sstatus>();
        sstatus.set_spie(sstatus.get_sie());
        sstatus.set_sie(0);

        let stvec = cpu.csr.get_by_type_existing::<Stvec>();
        cpu.write_pc(Self::next_pc_by_tvec(
            cause,
            stvec.get_mode(),
            stvec.get_base(),
        ));
    }

    pub fn sret(cpu: &mut RV32CPU) {
        let sstatus = cpu.csr.get_by_type::<Sstatus>().unwrap();
        sstatus.set_sie(sstatus.get_spie());
        sstatus.set_spie(1);
        let sepc = cpu.csr.get_by_type::<Sepc>().unwrap().get_sepc();
        cpu.write_pc(sepc);

        cpu.csr
            .set_current_privileged((sstatus.get_spp() as u8).into());
        sstatus.set_spp(0);
    }

    // ======================================
    //                 Common
    // ======================================

    pub fn try_send_trap_signal(cpu: &mut RV32CPU, cause: Trap, trap_value: WordType) -> bool {
        if cpu.csr.privelege_level() == PrivilegeLevel::M
            || Self::is_delegated_m_mode(cpu, cause) == false
        {
            match cause {
                Trap::Exception(_) => {
                    Self::send_trap_signal_m_mode(cpu, cause, trap_value);
                    true
                }
                Trap::Interrupt(_) => {
                    if cpu.csr.get_by_type_existing::<Mstatus>().get_mie() == 1 {
                        Self::send_trap_signal_m_mode(cpu, cause, trap_value);
                        true
                    } else {
                        false
                    }
                }
            }
        } else {
            match cause {
                Trap::Exception(_) => {
                    Self::send_trap_signal_s_mode(cpu, cause, trap_value);
                    true
                }
                Trap::Interrupt(_) => {
                    if cpu.csr.get_by_type_existing::<Mstatus>().get_sie() == 1 {
                        Self::send_trap_signal_s_mode(cpu, cause, trap_value);
                        true
                    } else {
                        false
                    }
                }
            }
        }
    }

    pub fn check_interrupt(cpu: &mut RV32CPU) -> Option<Interrupt> {
        let mstatus = cpu.csr.get_by_type::<Mstatus>().unwrap();
        if mstatus.get_mie() == 0 {
            return None;
        }

        let mip = cpu.csr.get_by_type::<Mip>().unwrap();
        let mie = cpu.csr.get_by_type::<Mie>().unwrap();

        let pending_interrupts = mip.data() & mie.data();

        if pending_interrupts == 0 {
            return None;
        }

        for i in 0..=15 {
            if (pending_interrupts & (1 << i)) != 0 {
                let interrupt = Interrupt::from(i as usize);
                return Some(interrupt);
            }
        }

        unreachable!()
    }

    /// Get the next pc value according to the trap vector (like `mtvec` or `stvec`).
    #[must_use]
    fn next_pc_by_tvec(cause: Trap, mode: WordType, base: WordType) -> WordType {
        match (mode, cause) {
            (0, _) | (1, Trap::Exception(_)) => {
                // Direct Mode
                base << 2
            }

            (1, Trap::Interrupt(ir)) => {
                // Vector Mode
                let offset: WordType = ir.into();
                (base << 2) + offset * 4
            }

            _ => {
                unreachable!()
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        isa::riscv::{cpu_tester::run_test_cpu_step, trap::Exception},
        ram_config,
    };

    const IRQ_HANDLER_ADDR: WordType = 0x80002000;

    #[test]
    fn test_load_fault() {
        run_test_cpu_step(
            &[0x00003503], // ld a0, 0(zero)
            |builder| builder.csr(csr_index::mtvec, IRQ_HANDLER_ADDR),
            |checker| {
                checker
                    .pc(IRQ_HANDLER_ADDR)
                    .csr(csr_index::mepc, ram_config::BASE_ADDR)
                    // .csr(csr_index::mtval, 0)
                    .customized(|checker| {
                        let mcause = checker.cpu.csr.get_by_type::<Mcause>().unwrap();
                        assert_eq!(mcause.get_interrupt(), 0);
                        assert_eq!(mcause.get_exception_code(), Exception::LoadFault.into());
                        checker
                    })
            },
        );
    }

    #[test]
    fn test_load_misaligned() {
        const BASE_LOAD_MEM: WordType = 0x80001000;
        run_test_cpu_step(
            &[0x0017B503], // ld a0, 1(a5)
            |builder| {
                builder
                    .csr(csr_index::mtvec, IRQ_HANDLER_ADDR)
                    .reg(15, BASE_LOAD_MEM)
            },
            |checker| {
                checker
                    .pc(IRQ_HANDLER_ADDR)
                    .csr(csr_index::mepc, ram_config::BASE_ADDR)
                    // .csr(csr_index::mtval, BASE_LOAD_MEM)
                    .customized(|checker| {
                        let mcause = checker.cpu.csr.get_by_type::<Mcause>().unwrap();
                        assert_eq!(mcause.get_interrupt(), 0);
                        assert_eq!(
                            mcause.get_exception_code(),
                            Exception::LoadMisaligned.into()
                        );
                        checker
                    })
            },
        );
    }

    #[test]
    fn test_store_fault() {
        run_test_cpu_step(
            &[0x00a7b023], // sd a0, 0(a5)
            |builder| builder.csr(csr_index::mtvec, IRQ_HANDLER_ADDR | 0b00),
            |checker| {
                checker
                    .pc(IRQ_HANDLER_ADDR)
                    .csr(csr_index::mepc, ram_config::BASE_ADDR)
                    .customized(|checker| {
                        let mcause = checker.cpu.csr.get_by_type::<Mcause>().unwrap();
                        assert_eq!(mcause.get_interrupt(), 0);
                        assert_eq!(mcause.get_exception_code(), Exception::StoreFault.into());
                        checker
                    })
            },
        );
    }

    #[test]
    fn test_store_misaligned() {
        const BASE_STORE_MEM: WordType = 0x80001000;
        run_test_cpu_step(
            &[0x00a7b0a3], // sd a0, 1(a5)
            |builder| {
                builder
                    .csr(csr_index::mtvec, IRQ_HANDLER_ADDR)
                    .reg(15, BASE_STORE_MEM)
            },
            |checker| {
                checker
                    .pc(IRQ_HANDLER_ADDR)
                    .csr(csr_index::mepc, ram_config::BASE_ADDR)
                    // .csr(csr_index::mtval, BASE_LOAD_MEM)
                    .customized(|checker| {
                        let mcause = checker.cpu.csr.get_by_type::<Mcause>().unwrap();
                        assert_eq!(mcause.get_interrupt(), 0);
                        assert_eq!(
                            mcause.get_exception_code(),
                            Exception::StoreMisaligned.into()
                        );
                        checker
                    })
            },
        );
    }

    #[test]
    fn test_illegal_instr() {
        const PC_START: WordType = 0x80001000;
        run_test_cpu_step(
            &[0x00a7b023], // Any Instr. Because `PC` do not start as 0x80000000.
            |builder| {
                builder
                    .csr(csr_index::mtvec, IRQ_HANDLER_ADDR | 0b00)
                    .pc(PC_START)
            },
            |checker| {
                checker
                    .pc(IRQ_HANDLER_ADDR)
                    .csr(csr_index::mepc, PC_START)
                    .customized(|checker| {
                        let mcause = checker.cpu.csr.get_by_type::<Mcause>().unwrap();
                        assert_eq!(mcause.get_interrupt(), 0);
                        assert_eq!(
                            mcause.get_exception_code(),
                            Exception::IllegalInstruction.into()
                        );
                        checker
                    })
            },
        );
    }

    #[test]
    fn test_instr_fault() {
        const PC_START: WordType = 0x70000000;
        run_test_cpu_step(
            &[0x00a7b023], // Any Instr. Because `PC` do not start as 0x80000000.
            |builder| {
                builder
                    .csr(csr_index::mtvec, IRQ_HANDLER_ADDR | 0b00)
                    .pc(PC_START)
            },
            |checker| {
                checker
                    .pc(IRQ_HANDLER_ADDR)
                    .csr(csr_index::mepc, PC_START)
                    .customized(|checker| {
                        let mcause = checker.cpu.csr.get_by_type::<Mcause>().unwrap();
                        assert_eq!(mcause.get_interrupt(), 0);
                        assert_eq!(
                            mcause.get_exception_code(),
                            Exception::InstructionFault.into()
                        );
                        checker
                    })
            },
        );
    }

    #[test]
    fn test_instr_misaligned() {
        const PC_START: WordType = 0x80000001;
        run_test_cpu_step(
            &[0x00a7b023], // Any Instr. Because `PC` do not start as 0x80000000.
            |builder| {
                builder
                    .csr(csr_index::mtvec, IRQ_HANDLER_ADDR | 0b00)
                    .pc(PC_START)
            },
            |checker| {
                checker
                    .pc(IRQ_HANDLER_ADDR)
                    .csr(csr_index::mepc, PC_START)
                    .customized(|checker| {
                        let mcause = checker.cpu.csr.get_by_type::<Mcause>().unwrap();
                        assert_eq!(mcause.get_interrupt(), 0);
                        assert_eq!(
                            mcause.get_exception_code(),
                            Exception::InstructionMisaligned.into()
                        );
                        checker
                    })
            },
        );
    }

    #[test]
    fn test_swap() {
        run_test_cpu_step(
            &[0x34011173], // csrrw sp, mscratch, sp
            |builder| builder.csr(csr_index::mscratch, 0x114514).reg(2, 0x0721),
            |checker| checker.csr(csr_index::mscratch, 0x0721).reg(2, 0x114514),
        );
    }

    #[test]
    fn test_next_pc_by_tvec() {
        let base = 0x12345 as WordType;
        assert_eq!(
            TrapController::next_pc_by_tvec(Trap::Exception(Exception::InstructionFault), 1, base),
            base << 2
        );

        assert_eq!(
            TrapController::next_pc_by_tvec(Trap::Interrupt(Interrupt::MachineTimer), 1, base),
            (base << 2) + 0x1c
        );
    }
}
