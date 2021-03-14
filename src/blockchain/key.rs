use ethsign::SecretKey;
use web3::{
    signing::{
        self,
        Key,
    },
    types::{
        Address,
        H256,
    },
};

#[derive(Clone)]
pub struct PrivateKey {
    inner: SecretKey,
}

impl PrivateKey {
	pub fn new(inner: SecretKey) -> Self {
		Self {
			inner,
		}
	}
}

impl Key for PrivateKey {
    fn sign(&self, message: &[u8], chain_id: Option<u64>) -> Result<signing::Signature, signing::SigningError> {
        let signature = self
            .inner
            .sign(message)
            .map_err(|_| signing::SigningError::InvalidMessage)?;

		let standard_v = signature.v as u64;
		let v = if let Some(chain_id) = chain_id {
			standard_v + 35 + chain_id * 2
		} else {
			standard_v + 27
		};
        Ok(signing::Signature {
            r: H256::from(signature.r),
            s: H256::from(signature.s),
            v,
        })
    }

    fn address(&self) -> web3::types::Address {
        Address::from(self.inner.public().address())
    }
}
