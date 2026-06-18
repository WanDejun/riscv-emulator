use crate::isa::{
    riscv::{
        RawInstr,
        decoder::{DecodeInstr, decode_info},
        instruction::{
            InstrFormat,
            instr_table::{RVInstrDesc, RiscvInstr},
        },
    },
    utils::DecodeMask,
};

pub(super) struct MaskDecoder {
    masks: Vec<(DecodeMask, RiscvInstr, InstrFormat)>,
}

impl MaskDecoder {
    pub fn decode(&self, raw: RawInstr) -> Option<DecodeInstr> {
        for (mask, instr, fmt) in self.masks.iter() {
            if mask.matches(raw.val) {
                return Some(DecodeInstr(*instr, decode_info(raw.val, *instr, *fmt)));
            }
        }

        None
    }

    pub fn from_isa(instrs: impl Iterator<Item = RVInstrDesc>) -> Self {
        let mut masks = vec![];
        for desc in instrs {
            masks.push((
                DecodeMask {
                    mask: desc.mask,
                    key: desc.key,
                },
                desc.instr,
                desc.format,
            ));
        }

        log::info!("Mask decoder loads {} instructions", masks.len());

        Self { masks }
    }
}
