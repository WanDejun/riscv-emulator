use crate::{
    config::arch_config::WordType,
    isa::{
        DebugTarget,
        riscv::{
            csr_reg::{
                PrivilegeLevel, csr_index,
                csr_macro::{Mstatus, Mtvec},
            },
            executor::RV32CPU,
            trap::Trap,
        },
    },
};

pub(in crate::isa::riscv) struct TrapController {}

impl TrapController {
    pub fn new() -> Self {
        Self {}
    }

    pub fn send_trap_signal(cpu: &mut RV32CPU, cause: Trap, pc: WordType, trap_value: WordType) {
        cpu.csr.write(csr_index::mcause, cause.into());
        cpu.csr.write(csr_index::mepc, pc);
        cpu.csr.write(csr_index::mtval, trap_value);
        let mstatus = cpu.csr.get_by_type::<Mstatus>();
        mstatus.set_mpie(mstatus.get_mie());
        mstatus.set_mie(0);
        cpu.csr.set_current_privileged(PrivilegeLevel::M);

        let mtvec = cpu.csr.get_by_type::<Mtvec>();
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
        let mstatus = cpu.csr.get_by_type::<Mstatus>();
        cpu.csr
            .set_current_privileged((mstatus.get_mpp() as u8).into());
        mstatus.set_mpp(0);
        mstatus.set_mie(mstatus.get_mpie());
        mstatus.set_mpie(1);

        cpu.write_pc(cpu.csr.read(csr_index::mepc).unwrap());
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
                        let mcause = checker.cpu.csr.get_by_type::<Mcause>();
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
                        let mcause = checker.cpu.csr.get_by_type::<Mcause>();
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
                        let mcause = checker.cpu.csr.get_by_type::<Mcause>();
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
                        let mcause = checker.cpu.csr.get_by_type::<Mcause>();
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
                        let mcause = checker.cpu.csr.get_by_type::<Mcause>();
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
                        let mcause = checker.cpu.csr.get_by_type::<Mcause>();
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
                        let mcause = checker.cpu.csr.get_by_type::<Mcause>();
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
}
