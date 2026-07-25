use crate::device::{PlicDeviceHandler, plic::PeriphIrqId};

pub trait PlicIRQHandler {
    fn handle_irq(&mut self, interrupt: PeriphIrqId, level: bool);
    fn register_source_handler(
        &mut self,
        source_handler: Option<Box<dyn PlicDeviceHandler>>,
        interrupt_id: PeriphIrqId,
    );
}

pub trait PlicIRQSource {
    fn set_irq_line(&mut self, target: *mut dyn PlicIRQHandler, interrupt_id: PeriphIrqId);
}

/// NOTE: Only used in single-threaded contexts.
pub struct PlicIRQLine {
    target: *mut dyn PlicIRQHandler,
    interrupt_id: PeriphIrqId,
}

impl PlicIRQLine {
    pub fn new(
        target: *mut dyn PlicIRQHandler,
        source_handler: Option<Box<dyn PlicDeviceHandler>>,
        interrupt_id: PeriphIrqId,
    ) -> Self {
        unsafe {
            target
                .as_mut_unchecked()
                .register_source_handler(source_handler, interrupt_id);
        }

        Self {
            target,
            interrupt_id,
        }
    }

    pub fn set_irq(&mut self, level: bool) {
        unsafe { &mut *self.target }.handle_irq(self.interrupt_id, level);
    }
}
