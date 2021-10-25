use crate::storage;

#[derive(Debug)]
pub struct RpcRepository {
    address: url::Url,
}

impl RpcRepository {
    pub fn connect(addr: url::Url) -> Self {
        Self { address: addr }
    }
}

impl storage::Repository for RpcRepository {
    fn address(&self) -> url::Url {
        self.address.clone()
    }
}
