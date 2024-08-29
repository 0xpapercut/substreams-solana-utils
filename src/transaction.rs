use std::collections::HashMap;

use substreams_solana::pb::sf::solana::r#type::v1::ConfirmedTransaction;

use crate::pubkey::{Pubkey, PubkeyRef};
use crate::instruction::{WrappedInstruction, get_flattened_instructions};
use crate::spl_token::{TokenInstruction, TokenAccount, TOKEN_PROGRAM_ID};

use anyhow::{anyhow, Error};

/// Context that can provide enough information to process an instruction
pub struct TransactionContext<'a> {
    pub accounts: Vec<PubkeyRef<'a>>,
    pub token_accounts: HashMap<PubkeyRef<'a>, TokenAccount<'a>>,
    pub signature: String,
}

impl<'a> TransactionContext<'a> {
    fn new(transaction: &'a ConfirmedTransaction) -> Self {
        let accounts = transaction.resolved_accounts().iter().map(|x| PubkeyRef { 0: x }).collect();
        let signature = bs58::encode(transaction.transaction.as_ref().unwrap().signatures.get(0).unwrap()).into_string();
        Self {
            accounts,
            token_accounts: HashMap::new(),
            signature,
        }
    }

    pub fn build(transaction: &'a ConfirmedTransaction) -> Result<Self, &'static str> {
        let mut context = Self::new(transaction);

        for token_balance in &transaction.meta.as_ref().unwrap().pre_token_balances {
            let address = context.accounts[token_balance.account_index as usize].clone();
            let token_account = TokenAccount {
                address: address.clone(),
                mint: Pubkey::try_from_string(&token_balance.mint).unwrap(),
                owner: Pubkey::try_from_string(&token_balance.owner).unwrap(),
            };
            context.token_accounts.insert(address, token_account);
        }

        let instructions = get_flattened_instructions(transaction);
        for instruction in instructions {
            context.update(&instruction);
        }

        Ok(context)
    }

    fn update(&mut self, instruction: &WrappedInstruction) {
        if self.accounts[instruction.program_id_index() as usize] != TOKEN_PROGRAM_ID {
            return;
        }
        match TokenInstruction::unpack(&instruction.data()) {
            Ok(TokenInstruction::InitializeAccount) => {
                let token_account = parse_token_account(instruction, self, None);
                self.token_accounts.insert(token_account.address.clone(), token_account);
            }
            Ok(TokenInstruction::InitializeAccount2 { owner }) |
            Ok(TokenInstruction::InitializeAccount3 { owner }) => {
                let token_account = parse_token_account(instruction, self, Some(owner));
                self.token_accounts.insert(token_account.address.clone(), token_account);
            }
            _ => ()
        }
    }

    pub fn get_token_account(&self, address: &PubkeyRef<'a>) -> Option<&TokenAccount> {
        self.token_accounts.get(address)
    }
}

/// Parses the Initialize SPL Token Instruction and returns a TokenAccount
fn parse_token_account<'a>(instruction: &WrappedInstruction, context: &TransactionContext<'a>, owner: Option<Pubkey>) -> TokenAccount<'a> {
    let address = context.accounts[instruction.accounts()[0] as usize].clone();
    let mint = context.accounts[instruction.accounts()[1] as usize].to_pubkey().unwrap();
    let owner = match owner {
        Some(pubkey) => pubkey,
        None => context.accounts[instruction.accounts()[2] as usize].to_pubkey().unwrap(),
    };
    TokenAccount {
        address,
        mint,
        owner
    }
}

pub fn get_context<'a>(transaction: &'a ConfirmedTransaction) -> Result<TransactionContext<'a>, Error> {
    if let Some(_) = transaction.meta.as_ref().unwrap().err {
        return Err(anyhow!("Cannot get context of failed instruction."));
    }
    TransactionContext::build(transaction).map_err(|x| anyhow!(x))
}

pub fn get_signature(transaction: &ConfirmedTransaction) -> String {
    bs58::encode(transaction.transaction.as_ref().unwrap().signatures.get(0).unwrap()).into_string()
}
