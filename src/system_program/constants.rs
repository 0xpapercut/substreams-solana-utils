use crate::pubkey::Pubkey;
use lazy_static::lazy_static;

lazy_static! {
    pub static ref SYSTEM_PROGRAM_ID: Pubkey = Pubkey::from_string("11111111111111111111111111111111");
}
