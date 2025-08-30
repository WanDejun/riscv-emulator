use crate::{
    config::arch_config::WordType,
    isa::{
        DebugTarget,
        riscv::{
            csr_reg::{
                PrivilegeLevel, csr_index,
                csr_macro::{Medeleg, Mideleg, Mstatus, Mtvec},
            },
            executor::RV32CPU,
            trap::{Exception, Interrupt, Trap},
        },
    },
};

pub(in crate::isa::riscv) struct TrapController {}

impl TrapController {
    // pub fn new() -> Self {
    //     Self {}
    // }

    // ======================================
    //                M-Mode
    // ======================================
    fn m_mode_check_exception_delegate(cpu: &mut RV32CPU, exception: Exception) -> bool {
        let medeleg = cpu.csr.get_by_type::<Medeleg>().unwrap();
        let medeleg_val = medeleg.get_medeleg();
        if (medeleg_val & (1 << exception as u8)) != 0 {
            true
        } else {
            false
        }
    }

    fn m_mode_check_interrupt_delegate(cpu: &mut RV32CPU, interrupt: Interrupt) -> bool {
        let mideleg = cpu.csr.get_by_type::<Mideleg>().unwrap();
        match interrupt {
            Interrupt::MachineExternal | Interrupt::MachineSoft | Interrupt::MachineTimer => false,
            Interrupt::SupervisorExternal | Interrupt::UserExternal => {
                if mideleg.get_seip() != 0 {
                    true
                } else {
                    false
                }
            }
            Interrupt::SupervisorSoft | Interrupt::UserSoft => {
                if mideleg.get_ssip() != 0 {
                    true
                } else {
                    false
                }
            }
            Interrupt::SupervisorTimer | Interrupt::UserTimer => {
                if mideleg.get_stip() != 0 {
                    true
                } else {
                    false
                }
            }
            Interrupt::Unknown => {
                unreachable!()
            }
        }
    }

    fn m_mode_check_delegate(cpu: &mut RV32CPU, cause: Trap) -> bool {
        match cause {
            Trap::Interrupt(interrupt) => Self::m_mode_check_interrupt_delegate(cpu, interrupt),
            Trap::Exception(exception) => Self::m_mode_check_exception_delegate(cpu, exception),
        }
    }

    fn m_mode_send_trap_signal(cpu: &mut RV32CPU, cause: Trap, trap_value: WordType) {
        cpu.csr.set_current_privileged(PrivilegeLevel::M);
        cpu.csr
            .write_uncheck_privilege(csr_index::mcause, cause.into());
        cpu.csr.write_uncheck_privilege(csr_index::mepc, cpu.pc);

        if matches!(
            cause, // TODO: Do we need to handle other exceptions?
            Trap::Exception(
                Exception::LoadMisaligned
                    | Exception::StoreMisaligned
                    | Exception::LoadFault
                    | Exception::StoreFault
            )
        ) == false
        {
            // Addr has been stored to mtval in `exec_load`/`exec_store` on mem error
            cpu.csr
                .write_uncheck_privilege(csr_index::mtval, trap_value);
        }

        let mstatus = cpu.csr.get_by_type::<Mstatus>().unwrap();
        mstatus.set_mpie(mstatus.get_mie());
        mstatus.set_mie(0);

        let mtvec = cpu.csr.get_by_type::<Mtvec>().unwrap();
        if mtvec.get_mode() == 0 {
            // Direct Mode
            cpu.write_pc(mtvec.get_base() << 2);
        } else {
            let offset: WordType = match cause {
                Trap::Exception(nr) => nr.into(),
                Trap::Interrupt(nr) => nr.into(),
            };
            cpu.write_pc((mtvec.get_base() << 2) + offset * size_of::<WordType>() as WordType);
        }
    }

    pub fn mret(cpu: &mut RV32CPU) {
        let mstatus = cpu.csr.get_by_type::<Mstatus>().unwrap();
        mstatus.set_mie(mstatus.get_mpie());
        mstatus.set_mpie(1);
        cpu.write_pc(cpu.csr.read_uncheck_privilege(csr_index::mepc).unwrap());

        cpu.csr
            .set_current_privileged((mstatus.get_mpp() as u8).into());
        mstatus.set_mpp(0);
    }

    pub fn send_trap_signal(cpu: &mut RV32CPU, cause: Trap, trap_value: WordType) {
        Self::m_mode_send_trap_signal(cpu, cause, trap_value);
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        isa::riscv::{cpu_tester::run_test_cpu_step, csr_reg::csr_macro::*, trap::Exception},
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
}
