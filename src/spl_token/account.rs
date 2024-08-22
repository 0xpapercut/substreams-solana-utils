use crate::pubkey::{Pubkey, PubkeyRef};

#[derive(Clone)]
pub struct TokenAccount<'a> {
    pub address: PubkeyRef<'a>,
    pub mint: Pubkey,
    pub owner: Pubkey,
}
