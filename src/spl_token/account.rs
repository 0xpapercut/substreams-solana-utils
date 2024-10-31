use crate::pubkey::{Pubkey, PubkeyRef};

#[derive(Clone, Debug)]
pub struct TokenAccount<'a> {
    pub address: PubkeyRef<'a>,
    pub mint: Pubkey,
    pub owner: Pubkey,
    pub pre_balance: Option<u64>,
    pub post_balance: Option<u64>,
}
