mod exec_atomic_function;
mod exec_float_function;

pub(super) mod exec_function;
pub mod exec_mapping;
pub mod instr_table;

use crate::{
    config::arch_config::WordType,
    isa::riscv::{
        self,
        csr_reg::csr_macro::{Minstret, Mstatus},
        executor::RVCPU,
        instruction::exec_function::save_fflags_to_cpu,
    },
};

/// A helper function for normal instruction execution.
///
/// It takes a closure `f` that performs the actual instruction logic.
/// If `f` executes successfully, it will increase PC by 4 and increase the Minstret CSR by 1.
#[inline(always)]
pub(super) fn normal_exec<F>(cpu: &mut RVCPU, f: F) -> Result<(), riscv::trap::Exception>
where
    F: FnOnce(&mut RVCPU) -> Result<(), riscv::trap::Exception>,
{
    f(cpu)?;
    cpu.pc = cpu.pc.wrapping_add(4);
    cpu.csr.get_by_type_existing::<Minstret>().wrapping_add(1);
    Ok(())
}

/// A helper function for normal floating-point instruction execution.
///
/// It first checks if the floating-point unit is enabled by examining the FS field in the Mstatus CSR.
///
/// If the FS field is 0, it returns an illegal instruction exception.
/// Otherwise, it calls [`normal_exec`].
#[inline(always)]
pub(super) fn normal_float_exec<F>(cpu: &mut RVCPU, f: F) -> Result<(), riscv::trap::Exception>
where
    F: FnOnce(&mut RVCPU) -> Result<(), riscv::trap::Exception>,
{
    if cpu.csr.get_by_type_existing::<Mstatus>().get_fs() == 0 {
        return Err(riscv::trap::Exception::IllegalInstruction);
    }

    normal_exec(cpu, f)?;

    save_fflags_to_cpu(cpu);

    Ok(())
}

type ExecFn = fn(RVInstrInfo, &mut RVCPU) -> Result<(), riscv::trap::Exception>;

/// `imm` value is shifted:
///
/// Type B: 1
/// Type U: 12
/// Type J: 12
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RVInstrInfo {
    None,
    R {
        rs1: u8,
        rs2: u8,
        rd: u8,
    },
    R_rm {
        rs1: u8,
        rs2: u8,
        rd: u8,
        rm: u8,
    },
    R4_rm {
        rs1: u8,
        rs2: u8,
        rs3: u8,
        rd: u8,
        rm: u8,
    },
    I {
        rs1: u8,
        rd: u8,
        imm: WordType,
    },
    S {
        rs1: u8,
        rs2: u8,
        imm: WordType,
    },
    B {
        rs1: u8,
        rs2: u8,
        imm: WordType,
    },
    U {
        rd: u8,
        imm: WordType,
    },
    J {
        rd: u8,
        imm: WordType,
    },
    A {
        rs1: u8,
        rs2: u8,
        rd: u8,
        rl: bool,
        aq: bool,
    },
}

#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy)]
pub enum InstrFormat {
    None,
    R,
    R_rm,
    R4_rm,
    I,
    S,
    B,
    U,
    J,
    A,
}

// define a single enum for every instruction
// define tables for each instruction set
#[macro_export]
macro_rules! define_riscv_isa {
    ( $tot_instr_name:ident,
        $( $isa_name:ident, $isa_table_name:ident, {$(
                $name:ident {
                    opcode: $opcode:literal,
                    funct3: $funct3:literal,
                    funct7: $funct7:literal,
                    format: $fmt:expr,
                    mask: $mask:literal,
                    key: $key:literal,
                    use_mask: $use_mask:literal,
                }),* $(,)?
            }
        ),* $(,)?
    ) => {

        define_instr_enum!($tot_instr_name, $($($name,)*)*);

        impl $tot_instr_name {
            pub fn isa_name(&self) -> &'static str {
                match self {
                    $(
                        $(
                            $tot_instr_name::$name => stringify!($isa_name),
                        )*
                    )*
                }
            }
        }

        #[derive(Debug, Clone)]
        pub struct RVInstrDesc {
            pub opcode: u8,
            pub funct3: u8,
            pub funct7: u8,
            pub instr: $tot_instr_name,
            pub format: InstrFormat,
            pub mask: u32,
            pub key: u32,
            pub use_mask: bool,
        }

        $(
            pub const $isa_table_name: &[RVInstrDesc] = &[
                $(
                    RVInstrDesc {
                        opcode: $opcode,
                        funct3: $funct3,
                        funct7: $funct7,
                        instr: $tot_instr_name::$name,
                        format: $fmt,
                        mask: $mask,
                        key: $key,
                        use_mask: $use_mask,
                    }
                ),*
            ];
        )*
    };
}
