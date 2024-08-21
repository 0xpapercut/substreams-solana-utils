use crate::pubkey::{Pubkey, PubkeyRef};

pub struct TokenAccount<'a> {
    pub address: PubkeyRef<'a>,
    pub mint: Pubkey,
    pub owner: Pubkey,
}
