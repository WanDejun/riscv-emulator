use crate::{
    config::arch_config::WordType,
    isa::riscv::{
        executor::RV32CPU,
        instruction::{RVInstrInfo, exec_function::*, rv32i_table::RiscvInstr},
        trap::Exception,
    },
    utils::sign_extend,
};

pub(in crate::isa::riscv) fn get_exec_func(
    instr: RiscvInstr,
) -> fn(RVInstrInfo, &mut RV32CPU) -> Result<(), Exception> {
    match instr {
        // Arith
        RiscvInstr::ADD | RiscvInstr::ADDI => exec_arith::<ExecAdd>,
        RiscvInstr::ADDW | RiscvInstr::ADDIW => exec_arith::<ExecAddw>,
        RiscvInstr::SUB => exec_arith::<ExecSub>,
        RiscvInstr::SUBW => exec_arith::<ExecSubw>,
        RiscvInstr::MUL => exec_arith::<ExecMulLow>,
        RiscvInstr::MULH => exec_arith::<ExecMulHighSighed>,
        RiscvInstr::MULHU => exec_arith::<ExecMulHighUnsigned>,
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
        RiscvInstr::SRA | RiscvInstr::SRAI | RiscvInstr::SRAW | RiscvInstr::SRAIW => {
            exec_arith::<ExecSRA>
        }

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
        RiscvInstr::JAL => |inst_info: RVInstrInfo, cpu: &mut RV32CPU| {
            if let RVInstrInfo::J { rd, imm } = inst_info {
                cpu.reg_file.write(rd, cpu.pc.wrapping_add(4));
                cpu.pc = cpu.pc.wrapping_add(sign_extend(imm, 21));
            } else {
                std::unreachable!();
            }
            Ok(())
        },

        RiscvInstr::JALR => |inst_info: RVInstrInfo, cpu: &mut RV32CPU| {
            if let RVInstrInfo::I { rs1, rd, imm } = inst_info {
                let t = cpu.pc + 4;
                let val = cpu.reg_file.read(rs1, 0).0;
                cpu.pc = (val.wrapping_add(sign_extend(imm, 12)) & !1) as WordType;
                cpu.reg_file.write(rd, t);
            } else {
                std::unreachable!();
            }

            Ok(())
        },

        RiscvInstr::AUIPC => |inst_info, cpu| {
            if let RVInstrInfo::U { rd, imm } = inst_info {
                cpu.reg_file
                    .write(rd, cpu.pc.wrapping_add(sign_extend(imm, 32)));
                cpu.pc = cpu.pc.wrapping_add(4);
                Ok(())
            } else {
                std::unreachable!();
            }
        },

        RiscvInstr::LUI => |inst_info, cpu| {
            if let RVInstrInfo::U { rd, imm } = inst_info {
                cpu.reg_file.write(rd, sign_extend(imm, 32));
                cpu.pc = cpu.pc.wrapping_add(4);
                Ok(())
            } else {
                std::unreachable!();
            }
        },

        RiscvInstr::EBREAK => |_info, _cpu| Err(Exception::Breakpoint),
        RiscvInstr::ECALL => |_info, _cpu| Err(Exception::MachineEnvCall),
        // todo!("Both ebreak and ecall are not implemented yet.")

        // We are executing in order, so don't need to do anything.
        RiscvInstr::FENCE => |_info, _cpu| Ok(()),

        RiscvInstr::CSRRW => exec_csrw::<false>,
        RiscvInstr::CSRRC => exec_csr_bit::<false, false>,
        RiscvInstr::CSRRS => exec_csr_bit::<true, false>,
        RiscvInstr::CSRRWI => exec_csrw::<true>,
        RiscvInstr::CSRRCI => exec_csr_bit::<false, true>,
        RiscvInstr::CSRRSI => exec_csr_bit::<true, true>,
    }
}
