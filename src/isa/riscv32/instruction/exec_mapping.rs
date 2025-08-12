use crate::{
    config::arch_config::WordType,
    isa::riscv32::{
        executor::RV32CPU,
        instruction::{Exception, RVInstrInfo, exec_function::*, rv32i_table::RiscvInstr},
    },
    utils::sign_extend,
};

pub(in crate::isa::riscv32) fn get_exec_func(
    instr: RiscvInstr,
) -> fn(RVInstrInfo, &mut RV32CPU) -> Result<(), Exception> {
    match instr {
        // Arith
        RiscvInstr::ADD | RiscvInstr::ADDI | RiscvInstr::ADDW | RiscvInstr::ADDIW => {
            |info, cpu| exec_arith::<ExecAdd>(info, cpu)
        }
        RiscvInstr::SUB | RiscvInstr::SUBW => |info, cpu| exec_arith::<ExecSub>(info, cpu),
        RiscvInstr::MUL => |info, cpu| exec_arith::<ExecMulLow>(info, cpu),
        RiscvInstr::MULH => |info, cpu| exec_arith::<ExecMulHighSighed>(info, cpu),
        RiscvInstr::MULHU => |info, cpu| exec_arith::<ExecMulHighUnsigned>(info, cpu),
        RiscvInstr::MULHSU => |info, cpu| exec_arith::<ExecMulHighSignedUnsigned>(info, cpu),
        RiscvInstr::DIV => |info, cpu| exec_arith::<ExecDivSigned>(info, cpu),
        RiscvInstr::DIVU => |info, cpu| exec_arith::<ExecDivUnsigned>(info, cpu),
        RiscvInstr::REM => |info, cpu| exec_arith::<ExecRemSigned>(info, cpu),
        RiscvInstr::REMU => |info, cpu| exec_arith::<ExecRemUnsigned>(info, cpu),
        RiscvInstr::MULW => |info, cpu| exec_arith::<ExecMulw>(info, cpu),
        RiscvInstr::DIVW => |info, cpu| exec_arith::<ExecDivw>(info, cpu),
        RiscvInstr::DIVUW => |info, cpu| exec_arith::<ExecDivuw>(info, cpu),
        RiscvInstr::REMW => |info, cpu| exec_arith::<ExecRemw>(info, cpu),
        RiscvInstr::REMUW => |info, cpu| exec_arith::<ExecRemuw>(info, cpu),

        // Shift
        RiscvInstr::SLL | RiscvInstr::SLLI => |info, cpu| exec_arith::<ExecSLL>(info, cpu),
        RiscvInstr::SRL | RiscvInstr::SRLI => |info, cpu| exec_arith::<ExecSRL>(info, cpu),
        RiscvInstr::SRA | RiscvInstr::SRAI | RiscvInstr::SRAW | RiscvInstr::SRAIW => {
            |info, cpu| exec_arith::<ExecSRA>(info, cpu)
        }

        RiscvInstr::SLLW | RiscvInstr::SLLIW => |info, cpu| exec_arith::<ExecSLLW>(info, cpu),
        RiscvInstr::SRLW | RiscvInstr::SRLIW => |info, cpu| exec_arith::<ExecSRLW>(info, cpu),

        // Cond set
        RiscvInstr::SLT | RiscvInstr::SLTI => |info, cpu| exec_arith::<ExecSignedLess>(info, cpu),
        RiscvInstr::SLTU | RiscvInstr::SLTIU => {
            |info, cpu| exec_arith::<ExecUnsignedLess>(info, cpu)
        }

        // Bit
        RiscvInstr::AND | RiscvInstr::ANDI => |info, cpu| exec_arith::<ExecAnd>(info, cpu),
        RiscvInstr::OR | RiscvInstr::ORI => |info, cpu| exec_arith::<ExecOr>(info, cpu),
        RiscvInstr::XOR | RiscvInstr::XORI => |info, cpu| exec_arith::<ExecXor>(info, cpu),

        // Branch
        RiscvInstr::BEQ => |info, cpu| exec_branch::<ExecEqual>(info, cpu),
        RiscvInstr::BNE => |info, cpu| exec_branch::<ExecNotEqual>(info, cpu),
        RiscvInstr::BLT => |info, cpu| exec_branch::<ExecSignedLess>(info, cpu),
        RiscvInstr::BGE => |info, cpu| exec_branch::<ExecSignedGreatEqual>(info, cpu),
        RiscvInstr::BLTU => |info, cpu| exec_branch::<ExecUnsignedLess>(info, cpu),
        RiscvInstr::BGEU => |info, cpu| exec_branch::<ExecUnsignedGreatEqual>(info, cpu),

        // Load
        RiscvInstr::LB => |info, cpu| exec_load::<u8, true>(info, cpu),
        RiscvInstr::LBU => |info, cpu| exec_load::<u8, false>(info, cpu),
        RiscvInstr::LH => |info, cpu| exec_load::<u16, true>(info, cpu),
        RiscvInstr::LHU => |info, cpu| exec_load::<u16, false>(info, cpu),
        RiscvInstr::LW => |info, cpu| exec_load::<u32, true>(info, cpu),
        RiscvInstr::LWU => |info, cpu| exec_load::<u32, false>(info, cpu),
        RiscvInstr::LD => |info, cpu| exec_load::<u64, false>(info, cpu),

        // Store
        RiscvInstr::SB => |info, cpu| exec_store::<u8>(info, cpu),
        RiscvInstr::SH => |info, cpu| exec_store::<u16>(info, cpu),
        RiscvInstr::SW => |info, cpu| exec_store::<u32>(info, cpu),
        RiscvInstr::SD => |info, cpu| exec_store::<u64>(info, cpu),

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

        RiscvInstr::EBREAK | RiscvInstr::ECALL => {
            todo!()
        }

        // We are executing in order, so don't need to do anything.
        RiscvInstr::FENCE => |_info, _cpu| Ok(()),
    }
}
