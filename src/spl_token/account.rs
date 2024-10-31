use crate::pubkey::{Pubkey, PubkeyRef};

#[derive(Clone, Debug)]
pub struct TokenAccount<'a> {
    pub address: PubkeyRef<'a>,
    pub mint: Pubkey,
    pub owner: Pubkey,
    pub balance: Option<u64>,
}
