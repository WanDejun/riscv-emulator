use crate::config::arch_config::{REGFILE_CNT, WordType};

pub struct FloatRegFile {
    data: [WordType; REGFILE_CNT],
}

impl FloatRegFile {
    pub fn new() -> Self {
        FloatRegFile {
            data: [0; REGFILE_CNT],
        }
    }

    pub fn read_float(&self, id1: u8, id2: u8) -> (f32, f32) {
        let float1 = f32::from_bits(self.data[id1 as usize] as u32);
        let float2 = f32::from_bits(self.data[id2 as usize] as u32);
        (float1, float2)
    }

    pub fn write_float(&mut self, id: u8, value: f32) {
        self.data[id as usize] = value.to_bits() as WordType;
    }

    #[cfg(feature = "riscv64")]
    pub fn read_double(&self, id1: u8, id2: u8) -> (f64, f64) {
        let double1 = f64::from_bits(self.data[id1 as usize]);
        let double2 = f64::from_bits(self.data[id2 as usize]);
        (double1, double2)
    }

    #[cfg(feature = "riscv64")]
    pub fn write_double(&mut self, id: u8, value: f64) {
        self.data[id as usize] = value.to_bits() as WordType;
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_float_regfile() {
        let mut regfile = FloatRegFile::new();
        regfile.write_float(0, 1.5);
        regfile.write_float(1, 2.5);
        assert_eq!(regfile.read_float(0, 1), (1.5, 2.5));

        regfile.write_double(2, 0.1);
        assert_eq!(regfile.read_double(2, 2), (0.1, 0.1));
        assert!(regfile.read_float(1, 0).0 != 0.0);
    }
}
