use crate::{
    config::arch_config::WordType,
    isa::riscv::{
        csr_reg::{
            PrivilegeLevel, csr_index,
            csr_macro::{Mstatus, Mtvec},
        },
        debugger::DebugTarget,
        executor::RV32CPU,
        trap::Trap,
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
