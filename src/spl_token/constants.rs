use crate::pubkey::Pubkey;
use lazy_static::lazy_static;

lazy_static! {
    pub static ref SOL_MINT: Pubkey = Pubkey::from_string("So11111111111111111111111111111111111111112");
    pub static ref TOKEN_PROGRAM_ID: Pubkey = Pubkey::from_string("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
}
