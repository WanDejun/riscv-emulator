use crate::isa::{
    riscv::{
        decoder::{DecoderTrait, decode_info},
        instruction::{
            InstrFormat, RVInstrInfo,
            rv32i_table::{RV32Desc, RiscvInstr},
        },
    },
    utils::DecodeMask,
};

pub(super) struct MaskDecoder {
    masks: Vec<(DecodeMask, RiscvInstr, InstrFormat)>,
}

impl DecoderTrait for MaskDecoder {
    fn decode(&self, raw_instr: u32) -> Option<(RiscvInstr, RVInstrInfo)> {
        for (mask, instr, fmt) in self.masks.iter() {
            if mask.matches(raw_instr) {
                return Some((*instr, decode_info(raw_instr, *instr, *fmt)));
            }
        }

        None
    }

    fn from_isa(instrs: &[RV32Desc]) -> Self {
        let mut masks = vec![];
        for desc in instrs {
            if desc.use_mask {
                masks.push((
                    DecodeMask {
                        mask: desc.mask,
                        key: desc.key,
                    },
                    desc.instr,
                    desc.format,
                ));
            }
        }

        Self { masks }
    }
}
