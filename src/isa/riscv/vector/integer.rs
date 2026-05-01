use crate::isa::riscv::{
    trap::Exception,
    vector::{
        Vector, Vsew,
        types::{VGFRef, VGFRefMut},
    },
};

impl Vector {
    fn exec_binary_integer<'a, Op>(
        &'a self,
        op: Op,
        vs1: u8,
        vs2: u8,
        vd: u8,
    ) -> Result<(), Exception>
    where
        Op: FnOnce(Vsew, VGFRef<'a>, VGFRef<'a>, VGFRefMut<'a>) -> Result<(), Exception>,
    {
        let (lmul, sew) = (self.config.lmul, self.config.sew);
        let vrf = &self.vector_regfile;
        let vs1_ref = VGFRef::new(sew.into(), vrf.read(lmul.into(), vs1).unwrap());
        let vs2_ref = VGFRef::new(sew.into(), vrf.read(lmul.into(), vs2).unwrap());
        let vd_ref = VGFRefMut::new(sew.into(), vrf.get_mut(lmul.into(), vd).unwrap());
        op(sew, vs1_ref, vs2_ref, vd_ref)
    }
}
