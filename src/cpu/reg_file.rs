use std::{
    fmt::Debug,
    ops::{Index, IndexMut},
};

use crate::config::arch_config::{REGFILE_CNT, WordType};

pub struct RegFile {
    data: [WordType; REGFILE_CNT],
}

impl Index<usize> for RegFile {
    type Output = WordType;

    fn index(&self, index: usize) -> &Self::Output {
        &self.data[index]
    }
}

impl IndexMut<usize> for RegFile {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.data[index]
    }
}

impl Debug for RegFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let byte_len = size_of::<WordType>();
        let hex_width = byte_len * 2; // 每字节2位16进制

        writeln!(f, "reg_file {{")?;
        for (i, val) in self.data.iter().enumerate() {
            // 每行8个
            if i % 8 == 0 {
                write!(f, "  ")?; // 缩进
            }

            write!(f, "x{:02}: 0x{:0width$x}  ", i, val, width = hex_width)?;

            if i % 8 == 7 {
                writeln!(f)?;
            }
        }

        // 若最后一行不足8个，手动换行
        if self.data.len() % 8 != 0 {
            writeln!(f)?;
        }

        write!(f, "}}")
    }
}

impl RegFile {
    pub fn new() -> Self {
        Self {
            data: [0; REGFILE_CNT],
        }
    }
}

#[cfg(test)]
mod tests {
    use rand::Rng;

    use super::*;

    #[test]
    fn test_fmt_output() {
        let mut rng = rand::rng();
        let mut reg = RegFile::new();

        for i in 0..32 {
            reg[i] = rng.random();
        }

        println!("reg = {:#?}", reg);
    }
}
