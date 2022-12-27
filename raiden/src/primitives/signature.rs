use web3::{signing::Signature, types::H256};

pub trait SignatureUtils {
    fn to_bytes(&self) -> Vec<u8>;
    fn to_h256(&self) -> H256;
}

impl SignatureUtils for Signature {
    fn to_bytes(&self) -> Vec<u8> {
        let vb = self.v.to_be_bytes();
        let rb = self.r.to_fixed_bytes();
        let sb = self.s.to_fixed_bytes();

        let mut b = vec![];
        b.extend(&vb);
        b.extend(&rb);
        b.extend(&sb);
        b
    }

    fn to_h256(&self) -> H256 {
        H256::from_slice(&self.to_bytes())
    }
}
