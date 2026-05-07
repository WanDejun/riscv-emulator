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
                RVInstrInfo, exec_atomic_function::*, exec_function::*, exec_vector_function::*,
                instr_table::RiscvInstr,
            },
            trap::{Exception, trap_controller::TrapController},
        },
    },
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
                let target = cpu.pc.wrapping_add(imm); // imm has been sign_extended

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
                let target: WordType = val.wrapping_add(imm) & !1; // imm has been sign_extended

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
                cpu.reg_file.write(rd, cpu.pc.wrapping_add(imm)); // imm has been sign_extended
                cpu.pc = cpu.pc.wrapping_add(4);
                cpu.csr.get_by_type_existing::<Minstret>().wrapping_add(1);
                Ok(())
            } else {
                std::unreachable!();
            }
        },

        RiscvInstr::LUI => |inst_info, cpu| {
            if let RVInstrInfo::U { rd, imm } = inst_info {
                cpu.reg_file.write(rd, imm); // imm has been sign_extended
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
            cpu.flush_icache();
            cpu.flush_tlb();
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
        RiscvInstr::LR_W => exec_lr::<u32, true>,
        RiscvInstr::SC_W => exec_sc::<u32>,

        RiscvInstr::LR_D => exec_lr::<u64, false>,
        RiscvInstr::SC_D => exec_sc::<u64>,

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

            cpu.memory.flush_tlb();
            cpu.flush_icache();

            cpu.write_pc(cpu.pc.wrapping_add(4));
            cpu.csr.get_by_type_existing::<Minstret>().wrapping_add(1);
            Ok(())
        },

        //---------------------------------------
        // RV_V
        //---------------------------------------
        //--------- config instruction. ---------
        RiscvInstr::VSETVL => exec_vector_config::<VsetvlFieldExtractor>,
        RiscvInstr::VSETVLI => exec_vector_config::<VsetvliFieldExtractor>,
        RiscvInstr::VSETIVLI => exec_vector_config::<VsetivliFieldExtractor>,

        //------- load store instruction. -------
        RiscvInstr::VLE8_V => vector_load::<0>,
        RiscvInstr::VLE16_V => vector_load::<1>,
        RiscvInstr::VLE32_V => vector_load::<2>,
        RiscvInstr::VLE64_V => vector_load::<3>,

        RiscvInstr::VSE8_V => vector_store::<0>,
        RiscvInstr::VSE16_V => vector_store::<1>,
        RiscvInstr::VSE32_V => vector_store::<2>,
        RiscvInstr::VSE64_V => vector_store::<3>,

        //-------- OPIVV (func3 = 0b000) --------
        RiscvInstr::VADD_VV => unimplemented!(), // Single-width Integer Arithmetic Instructions
        RiscvInstr::VSUB_VV => unimplemented!(),

        RiscvInstr::VMUL_VV => unimplemented!(), // Single-Width Integer Multiply Instructions
        RiscvInstr::VMULH_VV => unimplemented!(),
        RiscvInstr::VMULHU_VV => unimplemented!(),
        RiscvInstr::VMULHSU_VV => unimplemented!(),

        RiscvInstr::VDIV_VV => unimplemented!(), // Integer Divide Instructions
        RiscvInstr::VDIVU_VV => unimplemented!(),
        RiscvInstr::VREM_VV => unimplemented!(),
        RiscvInstr::VREMU_VV => unimplemented!(),

        RiscvInstr::VAND_VV => unimplemented!(), // Bitwise Logical Instructions
        RiscvInstr::VOR_VV => unimplemented!(),
        RiscvInstr::VXOR_VV => unimplemented!(),

        RiscvInstr::VSRA_VV => unimplemented!(), //  Single-Width Shift Instructions
        RiscvInstr::VSRL_VV => unimplemented!(),
        RiscvInstr::VSLL_VV => unimplemented!(),

        RiscvInstr::VNSRL_WV => unimplemented!(), // Widening Shift Instructions
        RiscvInstr::VNSRA_WV => unimplemented!(),

        RiscvInstr::VMSEQ_VV => unimplemented!(), // Integer Compare Instructions
        RiscvInstr::VMSNE_VV => unimplemented!(),
        RiscvInstr::VMSLTU_VV => unimplemented!(),
        RiscvInstr::VMSLT_VV => unimplemented!(),
        RiscvInstr::VMSLEU_VV => unimplemented!(),
        RiscvInstr::VMSLE_VV => unimplemented!(),

        RiscvInstr::VADC_VVM => unimplemented!(), // Add-with-Carry / Subtract-with-Borrow
        RiscvInstr::VMADC_VV => unimplemented!(),
        RiscvInstr::VMADC_VVM => unimplemented!(),
        RiscvInstr::VSBC_VVM => unimplemented!(),
        RiscvInstr::VMSBC_VV => unimplemented!(),
        RiscvInstr::VMSBC_VVM => unimplemented!(),

        RiscvInstr::VMAX_VV => unimplemented!(), // Integer Min/Max Instructions
        RiscvInstr::VMAXU_VV => unimplemented!(),
        RiscvInstr::VMIN_VV => unimplemented!(),
        RiscvInstr::VMINU_VV => unimplemented!(),

        //  Single-Width Integer Multiply-Add Instructions
        RiscvInstr::VMACC_VV => unimplemented!(), // vd[i] = (vs1[i] * vs2[i]) + vd[i]
        RiscvInstr::VNMSAC_VV => unimplemented!(), // vd[i] = -(vs1[i] * vs2[i]) + vd[i]
        RiscvInstr::VMADD_VV => unimplemented!(), // vd[i] = (vs1[i] * vd[i]) + vs2[i]
        RiscvInstr::VNMSUB_VV => unimplemented!(), // vd[i] = -(vs1[i] * vd[i]) + vs2[i]

        RiscvInstr::VMERGE_VVM | RiscvInstr::VMV_V_V => {
            |inst_info: RVInstrInfo, _cpu: &mut RVCPU| {
                if let RVInstrInfo::V { vm, .. } = inst_info {
                    if vm {
                        unimplemented!() // Handle VMV_V_V
                    } else {
                        unimplemented!() // Handle VMERGE_VVM
                    }
                } else {
                    std::unreachable!();
                }
            }
        }

        RiscvInstr::VSADDU_VV => unimplemented!(), // Single-Width Saturating Add and Subtract
        RiscvInstr::VSADD_VV => unimplemented!(),
        RiscvInstr::VSSUBU_VV => unimplemented!(),
        RiscvInstr::VSSUB_VV => unimplemented!(),

        RiscvInstr::VAADDU_VV => unimplemented!(), // Single-Width Averaging Add and Subtract
        RiscvInstr::VAADD_VV => unimplemented!(),
        RiscvInstr::VASUBU_VV => unimplemented!(),
        RiscvInstr::VASUB_VV => unimplemented!(),

        RiscvInstr::VSMUL_VV => unimplemented!(), //  Single-Width Fractional Multiply with Rounding and Saturation

        RiscvInstr::VSSRL_VV => unimplemented!(), // Single-Width Scaling Shift Instructions
        RiscvInstr::VSSRA_VV => unimplemented!(),

        RiscvInstr::VNCLIPU_WV => unimplemented!(), // Narrowing Fixed-Point Clip Instructions
        RiscvInstr::VNCLIP_WV => unimplemented!(),

        RiscvInstr::VRGATHER_VV => unimplemented!(), // Vector Gather Instructions
        RiscvInstr::VRGATHEREI16_VV => unimplemented!(),

        //-------- OPIVX (0x100) --------
        RiscvInstr::VADD_VX => unimplemented!(), // Single-width Integer Arithmetic Instructions
        RiscvInstr::VSUB_VX => unimplemented!(),
        RiscvInstr::VRSUB_VX => unimplemented!(),

        RiscvInstr::VMUL_VX => unimplemented!(), // Single-Width Integer Multiply Instructions
        RiscvInstr::VMULH_VX => unimplemented!(),
        RiscvInstr::VMULHU_VX => unimplemented!(),
        RiscvInstr::VMULHSU_VX => unimplemented!(),

        RiscvInstr::VDIV_VX => unimplemented!(), // Integer Divide Instructions
        RiscvInstr::VDIVU_VX => unimplemented!(),
        RiscvInstr::VREM_VX => unimplemented!(),
        RiscvInstr::VREMU_VX => unimplemented!(),

        RiscvInstr::VAND_VX => unimplemented!(), // Bitwise Logical Instructions
        RiscvInstr::VOR_VX => unimplemented!(),
        RiscvInstr::VXOR_VX => unimplemented!(),

        RiscvInstr::VSRA_VX => unimplemented!(), //  Single-Width Shift Instructions
        RiscvInstr::VSRL_VX => unimplemented!(),
        RiscvInstr::VSLL_VX => unimplemented!(),

        RiscvInstr::VNSRL_WX => unimplemented!(), // Widening Shift Instructions
        RiscvInstr::VNSRA_WX => unimplemented!(),

        RiscvInstr::VMSEQ_VX => unimplemented!(), // Integer Compare Instructions
        RiscvInstr::VMSNE_VX => unimplemented!(),
        RiscvInstr::VMSLTU_VX => unimplemented!(),
        RiscvInstr::VMSLT_VX => unimplemented!(),
        RiscvInstr::VMSLEU_VX => unimplemented!(),
        RiscvInstr::VMSLE_VX => unimplemented!(),
        RiscvInstr::VMSGTU_VX => unimplemented!(),
        RiscvInstr::VMSGT_VX => unimplemented!(),

        RiscvInstr::VADC_VXM => unimplemented!(), // Add-with-Carry / Subtract-with-Borrow
        RiscvInstr::VMADC_VX => unimplemented!(),
        RiscvInstr::VMADC_VXM => unimplemented!(),
        RiscvInstr::VSBC_VXM => unimplemented!(),
        RiscvInstr::VMSBC_VX => unimplemented!(),
        RiscvInstr::VMSBC_VXM => unimplemented!(),

        RiscvInstr::VMAX_VX => unimplemented!(), // Integer Min/Max Instructions
        RiscvInstr::VMAXU_VX => unimplemented!(),
        RiscvInstr::VMIN_VX => unimplemented!(),
        RiscvInstr::VMINU_VX => unimplemented!(),

        //  Single-Width Integer Multiply-Add Instructions
        RiscvInstr::VMACC_VX => unimplemented!(), // vd[i] = (vs1[i] * vs2[i]) + vd[i]
        RiscvInstr::VNMSAC_VX => unimplemented!(), // vd[i] = -(vs1[i] * vs2[i]) + vd[i]
        RiscvInstr::VMADD_VX => unimplemented!(), // vd[i] = (vs1[i] * vd[i]) + vs2[i]
        RiscvInstr::VNMSUB_VX => unimplemented!(), // vd[i] = -(vs1[i] * vd[i]) + vs2[i]

        RiscvInstr::VMERGE_VXM | RiscvInstr::VMV_V_X => {
            |inst_info: RVInstrInfo, _cpu: &mut RVCPU| {
                if let RVInstrInfo::V { vm, .. } = inst_info {
                    if vm {
                        unimplemented!() // Handle VMV_V_X
                    } else {
                        unimplemented!() // Handle VMERGE_VXM
                    }
                } else {
                    std::unreachable!();
                }
            }
        }

        RiscvInstr::VSADDU_VX => unimplemented!(), // Single-Width Saturating Add and Subtract
        RiscvInstr::VSADD_VX => unimplemented!(),
        RiscvInstr::VSSUBU_VX => unimplemented!(),
        RiscvInstr::VSSUB_VX => unimplemented!(),

        RiscvInstr::VAADDU_VX => unimplemented!(), // Single-Width Averaging Add and Subtract
        RiscvInstr::VAADD_VX => unimplemented!(),
        RiscvInstr::VASUBU_VX => unimplemented!(),
        RiscvInstr::VASUB_VX => unimplemented!(),

        RiscvInstr::VSMUL_VX => unimplemented!(), //  Single-Width Fractional Multiply with Rounding and Saturation

        RiscvInstr::VSSRL_VX => unimplemented!(), // Single-Width Scaling Shift Instructions
        RiscvInstr::VSSRA_VX => unimplemented!(),

        RiscvInstr::VNCLIPU_WX => unimplemented!(), // Narrowing Fixed-Point Clip Instructions
        RiscvInstr::VNCLIP_WX => unimplemented!(),

        RiscvInstr::VSLIDEUP_VX => unimplemented!(), // Vector Slide Instructions
        RiscvInstr::VSLIDEDOWN_VX => unimplemented!(),

        RiscvInstr::VRGATHER_VX => unimplemented!(), // Vector Gather Instructions

        //-------- OPIVI (func3 = 0b011) --------
        RiscvInstr::VADD_VI => unimplemented!(), // Single-width Integer Arithmetic Instructions
        RiscvInstr::VRSUB_VI => unimplemented!(),

        RiscvInstr::VAND_VI => unimplemented!(), // Bitwise Logical Instructions
        RiscvInstr::VOR_VI => unimplemented!(),
        RiscvInstr::VXOR_VI => unimplemented!(),

        RiscvInstr::VSRA_VI => unimplemented!(), //  Single-Width Shift Instructions
        RiscvInstr::VSRL_VI => unimplemented!(),
        RiscvInstr::VSLL_VI => unimplemented!(),

        RiscvInstr::VNSRL_WI => unimplemented!(), // Widening Shift Instructions
        RiscvInstr::VNSRA_WI => unimplemented!(),

        RiscvInstr::VMSEQ_VI => unimplemented!(), // Integer Compare Instructions
        RiscvInstr::VMSNE_VI => unimplemented!(),
        RiscvInstr::VMSLEU_VI => unimplemented!(),
        RiscvInstr::VMSLE_VI => unimplemented!(),
        RiscvInstr::VMSGTU_VI => unimplemented!(),
        RiscvInstr::VMSGT_VI => unimplemented!(),

        RiscvInstr::VADC_VIM => unimplemented!(), // Add-with-Carry / Subtract-with-Borrow
        RiscvInstr::VMADC_VI => unimplemented!(),
        RiscvInstr::VMADC_VIM => unimplemented!(),

        RiscvInstr::VMERGE_VIM | RiscvInstr::VMV_V_I => {
            |inst_info: RVInstrInfo, _cpu: &mut RVCPU| {
                if let RVInstrInfo::V { vm, .. } = inst_info {
                    if vm {
                        unimplemented!() // Handle VMV_V_X
                    } else {
                        unimplemented!() // Handle VMERGE_VXM
                    }
                } else {
                    std::unreachable!();
                }
            }
        }

        RiscvInstr::VSADDU_VI => unimplemented!(), // Single-Width Saturating Add and Subtract
        RiscvInstr::VSADD_VI => unimplemented!(),

        RiscvInstr::VSSRL_VI => unimplemented!(), // Single-Width Scaling Shift Instructions
        RiscvInstr::VSSRA_VI => unimplemented!(),

        RiscvInstr::VNCLIPU_WI => unimplemented!(), // Narrowing Fixed-Point Clip Instructions
        RiscvInstr::VNCLIP_WI => unimplemented!(),

        RiscvInstr::VSLIDEUP_VI => unimplemented!(), // Vector Slide Instructions
        RiscvInstr::VSLIDEDOWN_VI => unimplemented!(),

        RiscvInstr::VRGATHER_VI => unimplemented!(), // Vector Gather Instructions

        RiscvInstr::VMV1R_V => unimplemented!(), // Whole Vector Register Move
        RiscvInstr::VMV2R_V => unimplemented!(),
        RiscvInstr::VMV4R_V => unimplemented!(),
        RiscvInstr::VMV8R_V => unimplemented!(),

        //-------- OPMVV (func3 = 0b010) --------
        RiscvInstr::VWADD_VV => unimplemented!(), // Widening Integer Add/Subtract
        RiscvInstr::VWADD_WV => unimplemented!(),
        RiscvInstr::VWADDU_VV => unimplemented!(),
        RiscvInstr::VWADDU_WV => unimplemented!(),
        RiscvInstr::VWSUB_VV => unimplemented!(),
        RiscvInstr::VWSUB_WV => unimplemented!(),
        RiscvInstr::VWSUBU_VV => unimplemented!(),
        RiscvInstr::VWSUBU_WV => unimplemented!(),

        RiscvInstr::VWMUL_VV => unimplemented!(), // Widening Integer Multiply Instructions
        RiscvInstr::VWMULU_VV => unimplemented!(),
        RiscvInstr::VWMULSU_VV => unimplemented!(),

        RiscvInstr::VZEXT_VF2 => unimplemented!(), // Vector Sign/Zero Extension Instructions (for floating-point formats)
        RiscvInstr::VZEXT_VF4 => unimplemented!(),
        RiscvInstr::VZEXT_VF8 => unimplemented!(),
        RiscvInstr::VSEXT_VF2 => unimplemented!(),
        RiscvInstr::VSEXT_VF4 => unimplemented!(),
        RiscvInstr::VSEXT_VF8 => unimplemented!(),

        RiscvInstr::VWMACCU_VV => unimplemented!(), // Widening Integer Multiply-Add Instructions
        RiscvInstr::VWMACC_VV => unimplemented!(),
        RiscvInstr::VWMACCSU_VV => unimplemented!(),

        RiscvInstr::VCOMPRESS_VM => unimplemented!(), // Vector Compress, Expand, and Slide Instructions

        RiscvInstr::VMAND_MM => unimplemented!(), // Mask-Register Logical Instructions
        RiscvInstr::VMNAND_MM => unimplemented!(),
        RiscvInstr::VMANDN_MM => unimplemented!(),
        RiscvInstr::VMXOR_MM => unimplemented!(),
        RiscvInstr::VMOR_MM => unimplemented!(),
        RiscvInstr::VMNOR_MM => unimplemented!(),
        RiscvInstr::VMORN_MM => unimplemented!(),
        RiscvInstr::VMXNOR_MM => unimplemented!(),

        RiscvInstr::VCPOP_M => unimplemented!(), //count population in mask vcpop.m

        RiscvInstr::VFIRST_M => unimplemented!(), // find first set bit in mask vfirst.m

        RiscvInstr::VMSBF_M => unimplemented!(), // set-before-first mask bit
        RiscvInstr::VMSIF_M => unimplemented!(), // set-including-first mask bit
        RiscvInstr::VMSOF_M => unimplemented!(), // set-only-first mask bit

        RiscvInstr::VIOTA_M => unimplemented!(), // Iota Instruction
        RiscvInstr::VID_V => unimplemented!(),   //  Element Index Instruction

        RiscvInstr::VMV_S_X => unimplemented!(), // Vector Slide Instructions
        RiscvInstr::VMV_X_S => unimplemented!(),
        // RiscvInstr::VFMV_S_F => unimplemented!(),
        // RiscvInstr::VFMV_F_S => unimplemented!(),

        //-------- OPMVX (func3 = 0b110) --------
        RiscvInstr::VWADD_VX => unimplemented!(), // Widening Integer Add/Subtract
        RiscvInstr::VWADD_WX => unimplemented!(),
        RiscvInstr::VWADDU_VX => unimplemented!(),
        RiscvInstr::VWADDU_WX => unimplemented!(),
        RiscvInstr::VWSUB_VX => unimplemented!(),
        RiscvInstr::VWSUB_WX => unimplemented!(),
        RiscvInstr::VWSUBU_VX => unimplemented!(),
        RiscvInstr::VWSUBU_WX => unimplemented!(),

        RiscvInstr::VWMUL_VX => unimplemented!(), // Widening Integer Multiply Instructions
        RiscvInstr::VWMULU_VX => unimplemented!(),
        RiscvInstr::VWMULSU_VX => unimplemented!(),

        RiscvInstr::VWMACCU_VX => unimplemented!(), // Widening Integer Multiply-Add Instructions
        RiscvInstr::VWMACC_VX => unimplemented!(),
        RiscvInstr::VWMACCSU_VX => unimplemented!(),
        RiscvInstr::VWMACCUS_VX => unimplemented!(),
        //-------- OPFVV (func3 = 0b001) --------
        //-------- OPFVF (func3 = 0b101) --------
        // _ => unimplemented!(),
    }
}
