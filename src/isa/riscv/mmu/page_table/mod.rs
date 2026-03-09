#[cfg(feature = "riscv64")]
pub mod riscv64;
#[cfg(feature = "riscv64")]
pub use riscv64::*;

#[derive(Debug, PartialEq, Eq)]
pub enum PageTableError {
    AlignFault,
    PageFault,
    PrivilegeFault,
}
