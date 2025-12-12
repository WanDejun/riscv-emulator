use std::hint::unlikely;

use crate::{
    config::arch_config::WordType,
    fpu::soft_float::*,
    isa::{
        DebugTarget,
        riscv::{
            csr_reg::{
                PrivilegeLevel,
                csr_macro::{Minstret, Mstatus},
            },
            executor::RVCPU,
            instruction::{
                RVInstrInfo, exec_atomic_function::*, exec_function::*, instr_table::RiscvInstr,
            },
            trap::{Exception, trap_controller::TrapController},
        },
    },
    utils::sign_extend,
};

pub(in crate::isa::riscv) fn get_exec_func(
    instr: RiscvInstr,
) -> fn(RVInstrInfo, &mut RVCPU) -> Result<(), Exception> {
    match instr {
        //---------------------------------------
        // RV_I
        //---------------------------------------

        // Arith
        RiscvInstr::ADD | RiscvInstr::ADDI => exec_arith::<ExecAdd>,
        RiscvInstr::ADDW | RiscvInstr::ADDIW => exec_arith::<ExecAddw>,
        RiscvInstr::SUB => exec_arith::<ExecSub>,
        RiscvInstr::SUBW => exec_arith::<ExecSubw>,

        RiscvInstr::MUL => exec_arith::<ExecMulLow>,
        RiscvInstr::MULH => exec_arith::<ExecMulHighSigned<WordType>>,
        RiscvInstr::MULHU => exec_arith::<ExecMulHighUnsigned<WordType>>,
        RiscvInstr::MULHSU => exec_arith::<ExecMulHighSignedUnsigned>,
        RiscvInstr::DIV => exec_arith::<ExecDivSigned>,
        RiscvInstr::DIVU => exec_arith::<ExecDivUnsigned>,
        RiscvInstr::REM => exec_arith::<ExecRemSigned>,
        RiscvInstr::REMU => exec_arith::<ExecRemUnsigned>,
        RiscvInstr::MULW => exec_arith::<ExecMulw>,
        RiscvInstr::DIVW => exec_arith::<ExecDivw>,
        RiscvInstr::DIVUW => exec_arith::<ExecDivuw>,
        RiscvInstr::REMW => exec_arith::<ExecRemw>,
        RiscvInstr::REMUW => exec_arith::<ExecRemuw>,

        // Shift
        RiscvInstr::SLL | RiscvInstr::SLLI => exec_arith::<ExecSLL>,
        RiscvInstr::SRL | RiscvInstr::SRLI => exec_arith::<ExecSRL>,
        RiscvInstr::SRA | RiscvInstr::SRAI => exec_arith::<ExecSRA>,
        RiscvInstr::SRAW | RiscvInstr::SRAIW => exec_arith::<ExecSRAW>,

        RiscvInstr::SLLW | RiscvInstr::SLLIW => exec_arith::<ExecSLLW>,
        RiscvInstr::SRLW | RiscvInstr::SRLIW => exec_arith::<ExecSRLW>,

        // Cond set
        RiscvInstr::SLT | RiscvInstr::SLTI => exec_arith::<ExecSignedLess>,
        RiscvInstr::SLTU | RiscvInstr::SLTIU => exec_arith::<ExecUnsignedLess>,

        // Bit
        RiscvInstr::AND | RiscvInstr::ANDI => exec_arith::<ExecAnd>,
        RiscvInstr::OR | RiscvInstr::ORI => exec_arith::<ExecOr>,
        RiscvInstr::XOR | RiscvInstr::XORI => exec_arith::<ExecXor>,

        // Branch
        RiscvInstr::BEQ => exec_branch::<ExecEqual>,
        RiscvInstr::BNE => exec_branch::<ExecNotEqual>,
        RiscvInstr::BLT => exec_branch::<ExecSignedLess>,
        RiscvInstr::BGE => exec_branch::<ExecSignedGreatEqual>,
        RiscvInstr::BLTU => exec_branch::<ExecUnsignedLess>,
        RiscvInstr::BGEU => exec_branch::<ExecUnsignedGreatEqual>,

        // Load
        RiscvInstr::LB => exec_load::<u8, true>,
        RiscvInstr::LBU => exec_load::<u8, false>,
        RiscvInstr::LH => exec_load::<u16, true>,
        RiscvInstr::LHU => exec_load::<u16, false>,
        RiscvInstr::LW => exec_load::<u32, true>,
        RiscvInstr::LWU => exec_load::<u32, false>,
        RiscvInstr::LD => exec_load::<u64, false>,

        // Store
        RiscvInstr::SB => exec_store::<u8>,
        RiscvInstr::SH => exec_store::<u16>,
        RiscvInstr::SW => exec_store::<u32>,
        RiscvInstr::SD => exec_store::<u64>,

        // Jump and link
        RiscvInstr::JAL => |inst_info: RVInstrInfo, cpu: &mut RVCPU| {
            if let RVInstrInfo::J { rd, imm } = inst_info {
                let target = cpu.pc.wrapping_add(sign_extend(imm, 21));

                // > "The JAL and JALR instructions will generate an instruction-address-misaligned exception
                // if the target address is not aligned to a four-byte boundary."
                // TODO: Remember that this check in `JAL` and `JALR` should be disabled if 16-bit instructions are enabled.
                if unlikely((target & 0x3) != 0) {
                    return Err(Exception::InstructionMisaligned);
                }

                cpu.reg_file.write(rd, cpu.pc.wrapping_add(4));
                cpu.pc = target;
            } else {
                std::unreachable!();
            }
            cpu.csr.get_by_type_existing::<Minstret>().wrapping_add(1);
            Ok(())
        },

        RiscvInstr::JALR => |inst_info: RVInstrInfo, cpu: &mut RVCPU| {
            if let RVInstrInfo::I { rs1, rd, imm } = inst_info {
                let t = cpu.pc + 4;
                let val = cpu.reg_file.read(rs1, 0).0;
                let target: WordType = val.wrapping_add(sign_extend(imm, 12)) & !1;

                // Same as JAL
                if unlikely((target & 0x3) != 0) {
                    return Err(Exception::InstructionMisaligned);
                }

                cpu.pc = target;
                cpu.reg_file.write(rd, t);
            } else {
                std::unreachable!();
            }

            cpu.csr.get_by_type_existing::<Minstret>().wrapping_add(1);
            Ok(())
        },

        RiscvInstr::AUIPC => |inst_info, cpu| {
            if let RVInstrInfo::U { rd, imm } = inst_info {
                cpu.reg_file
                    .write(rd, cpu.pc.wrapping_add(sign_extend(imm, 32)));
                cpu.pc = cpu.pc.wrapping_add(4);
                cpu.csr.get_by_type_existing::<Minstret>().wrapping_add(1);
                Ok(())
            } else {
                std::unreachable!();
            }
        },

        RiscvInstr::LUI => |inst_info, cpu| {
            if let RVInstrInfo::U { rd, imm } = inst_info {
                cpu.reg_file.write(rd, sign_extend(imm, 32));
                cpu.pc = cpu.pc.wrapping_add(4);
                cpu.csr.get_by_type_existing::<Minstret>().wrapping_add(1);
                Ok(())
            } else {
                std::unreachable!();
            }
        },

        RiscvInstr::EBREAK => |_info, _cpu| Err(Exception::Breakpoint),
        RiscvInstr::ECALL => |_info, _cpu| match _cpu.get_current_privilege() {
            PrivilegeLevel::U => Err(Exception::UserEnvCall),
            PrivilegeLevel::S => Err(Exception::SupervisorEnvCall),
            PrivilegeLevel::M => Err(Exception::MachineEnvCall),
            _ => todo!(),
        },

        // We are executing in order, so don't need to do anything.
        RiscvInstr::FENCE => exec_nop,

        RiscvInstr::FENCE_I => |_info, cpu| {
            cpu.clear_all_cache();
            cpu.pc = cpu.pc.wrapping_add(4);
            cpu.csr.get_by_type_existing::<Minstret>().wrapping_add(1);
            Ok(())
        },

        RiscvInstr::CSRRW => exec_csrw::<false>,
        RiscvInstr::CSRRC => exec_csr_bit::<false, false>,
        RiscvInstr::CSRRS => exec_csr_bit::<true, false>,
        RiscvInstr::CSRRWI => exec_csrw::<true>,
        RiscvInstr::CSRRCI => exec_csr_bit::<false, true>,
        RiscvInstr::CSRRSI => exec_csr_bit::<true, true>,

        RiscvInstr::MRET => |_info, cpu| {
            if cpu.get_current_privilege() != PrivilegeLevel::M {
                return Err(Exception::IllegalInstruction);
            }
            TrapController::mret(cpu);

            cpu.csr.get_by_type_existing::<Minstret>().wrapping_add(1);
            Ok(())
        },
        RiscvInstr::WFI => exec_nop,

        //---------------------------------------
        // RV_F
        //---------------------------------------

        // Arith
        RiscvInstr::FADD_S => exec_float_arith_rm::<f32, AddOp>,
        RiscvInstr::FSUB_S => exec_float_arith_rm::<f32, SubOp>,
        RiscvInstr::FMUL_S => exec_float_arith_rm::<f32, MulOp>,
        RiscvInstr::FDIV_S => exec_float_arith_rm::<f32, DivOp>,
        RiscvInstr::FMADD_S => exec_float_arith_r4_rm::<f32, MulAddOp>,
        RiscvInstr::FNMADD_S => exec_float_arith_r4_rm::<f32, NegMulAddOp>,
        RiscvInstr::FMSUB_S => exec_float_arith_r4_rm::<f32, MulSubOp>,
        RiscvInstr::FNMSUB_S => exec_float_arith_r4_rm::<f32, NegMulSubOp>,
        RiscvInstr::FSQRT_S => exec_sqrt::<f32>,
        RiscvInstr::FMIN_S => exec_float_min::<f32>,
        RiscvInstr::FMAX_S => exec_float_max::<f32>,

        RiscvInstr::FADD_D => exec_float_arith_rm::<f64, AddOp>,
        RiscvInstr::FSUB_D => exec_float_arith_rm::<f64, SubOp>,
        RiscvInstr::FMUL_D => exec_float_arith_rm::<f64, MulOp>,
        RiscvInstr::FDIV_D => exec_float_arith_rm::<f64, DivOp>,
        RiscvInstr::FMADD_D => exec_float_arith_r4_rm::<f64, MulAddOp>,
        RiscvInstr::FNMADD_D => exec_float_arith_r4_rm::<f64, NegMulAddOp>,
        RiscvInstr::FMSUB_D => exec_float_arith_r4_rm::<f64, MulSubOp>,
        RiscvInstr::FNMSUB_D => exec_float_arith_r4_rm::<f64, NegMulSubOp>,
        RiscvInstr::FSQRT_D => exec_sqrt::<f64>,
        RiscvInstr::FMIN_D => exec_float_min::<f64>,
        RiscvInstr::FMAX_D => exec_float_max::<f64>,

        // Sign injection
        RiscvInstr::FSGNJ_S => exec_float_arith::<f32, SignInjectOp>,
        RiscvInstr::FSGNJN_S => exec_float_arith::<f32, SignInjectNegOp>,
        RiscvInstr::FSGNJX_S => exec_float_arith::<f32, SignInjectXorOp>,

        RiscvInstr::FSGNJ_D => exec_float_arith::<f64, SignInjectOp>,
        RiscvInstr::FSGNJN_D => exec_float_arith::<f64, SignInjectNegOp>,
        RiscvInstr::FSGNJX_D => exec_float_arith::<f64, SignInjectXorOp>,

        // Convert (RV32F)
        RiscvInstr::FCVT_W_S => exec_cvt_i_from_f::<f32, u32>,
        RiscvInstr::FCVT_WU_S => exec_cvt_u_from_f::<f32, u32>,
        RiscvInstr::FCVT_S_W => exec_cvt_f_from_i::<f32, 32>,
        RiscvInstr::FCVT_S_WU => exec_cvt_f_from_u::<f32, 32>,

        // Convert (RV64F/D)
        RiscvInstr::FCVT_L_S => exec_cvt_i_from_f::<f32, u64>,
        RiscvInstr::FCVT_LU_S => exec_cvt_u_from_f::<f32, u64>,
        RiscvInstr::FCVT_S_L => exec_cvt_f_from_i::<f32, 64>,
        RiscvInstr::FCVT_S_LU => exec_cvt_f_from_u::<f32, 64>,

        RiscvInstr::FCVT_W_D => exec_cvt_i_from_f::<f64, u32>,
        RiscvInstr::FCVT_WU_D => exec_cvt_u_from_f::<f64, u32>,
        RiscvInstr::FCVT_D_W => exec_cvt_f_from_i::<f64, 32>,
        RiscvInstr::FCVT_D_WU => exec_cvt_f_from_u::<f64, 32>,

        RiscvInstr::FCVT_L_D => exec_cvt_i_from_f::<f64, u64>,
        RiscvInstr::FCVT_LU_D => exec_cvt_u_from_f::<f64, u64>,
        RiscvInstr::FCVT_D_L => exec_cvt_f_from_i::<f64, 64>,
        RiscvInstr::FCVT_D_LU => exec_cvt_f_from_u::<f64, 64>,

        RiscvInstr::FCVT_D_S => exec_cvt_float::<f32, f64>,
        RiscvInstr::FCVT_S_D => exec_cvt_float::<f64, f32>,

        // Float Compare
        RiscvInstr::FEQ_S => exec_float_compare::<EqOp, f32>,
        RiscvInstr::FLT_S => exec_float_compare::<LtOp, f32>,
        RiscvInstr::FLE_S => exec_float_compare::<LeOp, f32>,

        RiscvInstr::FEQ_D => exec_float_compare::<EqOp, f64>,
        RiscvInstr::FLT_D => exec_float_compare::<LtOp, f64>,
        RiscvInstr::FLE_D => exec_float_compare::<LeOp, f64>,

        // Store/Load
        RiscvInstr::FLW => exec_float_load::<f32>,
        RiscvInstr::FSW => exec_float_store::<f32>,

        RiscvInstr::FLD => exec_float_load::<f64>,
        RiscvInstr::FSD => exec_float_store::<f64>,

        // Move
        RiscvInstr::FMV_X_W => exec_mv_x_from_f::<f32, true>,
        RiscvInstr::FMV_W_X => exec_mv_f_from_x::<f32>,

        RiscvInstr::FMV_X_D => exec_mv_x_from_f::<f64, false>,
        RiscvInstr::FMV_D_X => exec_mv_f_from_x::<f64>,

        // Classify
        RiscvInstr::FCLASS_S => exec_float_classify::<f32>,
        RiscvInstr::FCLASS_D => exec_float_classify::<f64>,

        //---------------------------------------
        // RV_A
        //---------------------------------------

        // arith
        RiscvInstr::AMOADD_W => exec_atomic_memory_operation::<ExecAmoAdd, u32>,
        RiscvInstr::AMOAND_W => exec_atomic_memory_operation::<ExecAmoAnd, u32>,
        RiscvInstr::AMOOR_W => exec_atomic_memory_operation::<ExecAmoOr, u32>,
        RiscvInstr::AMOXOR_W => exec_atomic_memory_operation::<ExecAmoXor, u32>,

        RiscvInstr::AMOADD_D => exec_atomic_memory_operation::<ExecAmoAdd, u64>,
        RiscvInstr::AMOAND_D => exec_atomic_memory_operation::<ExecAmoAnd, u64>,
        RiscvInstr::AMOOR_D => exec_atomic_memory_operation::<ExecAmoOr, u64>,
        RiscvInstr::AMOXOR_D => exec_atomic_memory_operation::<ExecAmoXor, u64>,

        // swap
        RiscvInstr::AMOSWAP_W => exec_atomic_memory_operation::<ExecAmoSwap, u32>,
        RiscvInstr::AMOSWAP_D => exec_atomic_memory_operation::<ExecAmoSwap, u64>,

        // cmp
        RiscvInstr::AMOMAX_W => exec_atomic_memory_operation::<ExecAmoMax, u32>,
        RiscvInstr::AMOMIN_W => exec_atomic_memory_operation::<ExecAmoMin, u32>,
        RiscvInstr::AMOMAXU_W => exec_atomic_memory_operation::<ExecAmoMaxU, u32>,
        RiscvInstr::AMOMINU_W => exec_atomic_memory_operation::<ExecAmoMinU, u32>,

        RiscvInstr::AMOMAX_D => exec_atomic_memory_operation::<ExecAmoMax, u64>,
        RiscvInstr::AMOMIN_D => exec_atomic_memory_operation::<ExecAmoMin, u64>,
        RiscvInstr::AMOMAXU_D => exec_atomic_memory_operation::<ExecAmoMaxU, u64>,
        RiscvInstr::AMOMINU_D => exec_atomic_memory_operation::<ExecAmoMinU, u64>,

        // load-reserved / store-conditional
        RiscvInstr::LR_W => todo!(),
        RiscvInstr::SC_W => todo!(),

        RiscvInstr::LR_D => todo!(),
        RiscvInstr::SC_D => todo!(),

        //---------------------------------------
        // RV_Custom
        //---------------------------------------
        #[cfg(feature = "custom-instr")]
        RiscvInstr::MY_INSTR0_R => todo!(),
        #[cfg(feature = "custom-instr")]
        RiscvInstr::MY_INSTR1_DISPLAY => |info, cpu| {
            if let RVInstrInfo::I { rs1: _, rd: _, imm } = info {
                print!("{}", imm as u8 as char);
            }

            cpu.pc = cpu.pc.wrapping_add(4);
            cpu.csr.get_by_type_existing::<Minstret>().wrapping_add(1);
            Ok(())
        },

        //---------------------------------------
        // RV_S
        //---------------------------------------
        RiscvInstr::SRET => |_info, cpu| {
            if cpu.get_current_privilege() < PrivilegeLevel::S {
                return Err(Exception::IllegalInstruction);
            }
            if cpu.get_current_privilege() == PrivilegeLevel::S
                && cpu.csr.get_by_type_existing::<Mstatus>().get_tsr() == 1
            {
                return Err(Exception::IllegalInstruction);
            }

            TrapController::sret(cpu);
            cpu.csr.get_by_type_existing::<Minstret>().wrapping_add(1);
            Ok(())
        },

        RiscvInstr::SFENCE_VMA => |_info, cpu| {
            if cpu.get_current_privilege() < PrivilegeLevel::S {
                return Err(Exception::IllegalInstruction);
            }
            if cpu.get_current_privilege() == PrivilegeLevel::S
                && cpu.csr.get_by_type_existing::<Mstatus>().get_tvm() == 1
            {
                return Err(Exception::IllegalInstruction);
            }

            cpu.clear_all_cache();
            cpu.write_pc(cpu.pc.wrapping_add(4));
            cpu.csr.get_by_type_existing::<Minstret>().wrapping_add(1);
            Ok(())
        },
    }
}
