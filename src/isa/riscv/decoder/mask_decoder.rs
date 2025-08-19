use crate::isa::{
    riscv::{
        RiscvTypes,
        decoder::{DecodeInstr, DecoderTrait, decode_info},
        instruction::{
            InstrFormat,
            rv32i_table::{RV32Desc, RiscvInstr},
        },
    },
    utils::DecodeMask,
};

pub(super) struct MaskDecoder {
    masks: Vec<(DecodeMask, RiscvInstr, InstrFormat)>,
}

impl DecoderTrait<RiscvTypes> for MaskDecoder {
    fn decode(&self, raw_instr: u32) -> Option<DecodeInstr> {
        for (mask, instr, fmt) in self.masks.iter() {
            if mask.matches(raw_instr) {
                return Some(DecodeInstr(*instr, decode_info(raw_instr, *instr, *fmt)));
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

        log::debug!("Mask decoder loads {} instructions", masks.len());

        Self { masks }
    }
}
