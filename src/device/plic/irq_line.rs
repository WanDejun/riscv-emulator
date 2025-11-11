use crate::device::plic::ExternalInterrupt;

pub trait PlicIRQHandler {
    fn handle_irq(&mut self, interrupt: ExternalInterrupt, level: bool);
}

pub trait PlicIRQSource {
    fn set_irq_line(&mut self, line: PlicIRQLine, id: usize);
}

/// NOTE: Only used in single-threaded contexts.
pub struct PlicIRQLine {
    target: *mut dyn PlicIRQHandler,
}

impl PlicIRQLine {
    pub fn new(target: *mut dyn PlicIRQHandler) -> Self {
        Self { target }
    }

    pub fn set_irq(&mut self, interrupt: ExternalInterrupt, level: bool) {
        unsafe { &mut *self.target }.handle_irq(interrupt, level);
    }
}
