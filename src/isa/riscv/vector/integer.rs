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
        let (vlmul, vsew) = (self.config.vlmul, self.config.vsew);
        let vrf = &self.vector_regfile;
        let vs1_ref = VGFRef::new(vrf.read(vlmul.get_lmul(), vs1).unwrap(), vsew.get_sew());
        let vs2_ref = VGFRef::new(vrf.read(vlmul.get_lmul(), vs2).unwrap(), vsew.get_sew());
        let vd_ref = VGFRefMut::new(
            vrf.get_mut(vlmul.get_lmul(), vd, 1).unwrap(),
            vsew.get_sew(),
            vlmul.get_lmul(),
            1,
        );
        op(vsew, vs1_ref, vs2_ref, vd_ref)
    }
}
