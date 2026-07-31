use crate::device::plic::PeriphIrqId;

pub trait PlicIRQHandler {
    fn handle_irq(&mut self, interrupt: PeriphIrqId, level: bool);
}

pub trait PlicIRQSource {
    fn set_irq_line(&mut self, target: *mut dyn PlicIRQHandler, interrupt_id: PeriphIrqId);
}

/// TODO: it's for legacy code, will be deprecated.
pub struct PlicIRQLine {
    target: *mut dyn PlicIRQHandler,
    interrupt_id: PeriphIrqId,
}

impl PlicIRQLine {
    pub fn new(target: *mut dyn PlicIRQHandler, interrupt_id: PeriphIrqId) -> Self {
        Self {
            target,
            interrupt_id,
        }
    }

    pub fn set_irq(&mut self, level: bool) {
        unsafe { &mut *self.target }.handle_irq(self.interrupt_id, level);
    }
}
