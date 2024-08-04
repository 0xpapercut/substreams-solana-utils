use std::collections::HashMap;

mod instruction;
use instruction::WrappedInstruction;
pub use instruction::{
    get_structured_instructions,
    get_flattened_instructions,
    StructuredInstruction,
    StructuredInstructions,
};

use substreams_solana::pb::sf::solana::r#type::v1::ConfirmedTransaction;
use substreams_solana_program_instructions::pubkey::Pubkey;

mod token;
pub use token::TokenAccount;

mod log;

use substreams_solana_spl_token as spl_token;

/// Context that can provide enough information to process an instruction
pub struct TransactionContext<'a> {
    pub accounts: Vec<&'a Vec<u8>>,
    pub token_accounts: HashMap<Vec<u8>, TokenAccount>,
    pub signature: String,
}

impl<'a> TransactionContext<'a> {
    fn new(transaction: &'a ConfirmedTransaction) -> Self {
        let accounts = transaction.resolved_accounts();
        let signature = bs58::encode(transaction.transaction.as_ref().unwrap().signatures.get(0).unwrap()).into_string();
        Self {
            accounts,
            token_accounts: HashMap::new(),
            signature,
        }
    }

    pub fn construct(transaction: &'a ConfirmedTransaction) -> Self {
        let mut context = Self::new(transaction);

        for token_balance in &transaction.meta.as_ref().unwrap().pre_token_balances {
            let address = context.get_account_from_index(token_balance.account_index as usize).clone();
            let mint = bs58::decode(&token_balance.mint).into_vec().unwrap();
            let owner = bs58::decode(&token_balance.owner).into_vec().unwrap();
            context.token_accounts.insert(address.clone(), TokenAccount {
                address,
                mint,
                owner
            });
        }

        let instructions = get_flattened_instructions(transaction);
        for instruction in instructions {
            context.update(&instruction);
        }

        context
    }

    fn update(&mut self, instruction: &WrappedInstruction) {
        let program = bs58::encode(self.accounts[instruction.program_id_index() as usize]).into_string();
        if program != spl_token::TOKEN_PROGRAM {
            return;
        }
        match spl_token::TokenInstruction::unpack(&instruction.data()) {
            Ok(spl_token::TokenInstruction::InitializeAccount) => {
                let token_account = parse_token_account(instruction, self, None);
                self.token_accounts.insert(token_account.address.clone(), token_account);
            }
            Ok(spl_token::TokenInstruction::InitializeAccount2 { owner }) |
            Ok(spl_token::TokenInstruction::InitializeAccount3 { owner }) => {
                let token_account = parse_token_account(instruction, self, Some(owner));
                self.token_accounts.insert(token_account.address.clone(), token_account);
            }
            _ => ()
        }
    }

    pub fn get_account_from_index(&'a self, index: usize) -> &Vec<u8> {
        self.accounts[index]
    }

    pub fn get_token_account_from_address(&'a self, address: &Vec<u8>) -> Option<&TokenAccount> {
        self.token_accounts.get(address)
    }

    pub fn get_token_account_from_index(&'a self, index: usize) -> &TokenAccount {
        &self.token_accounts[self.accounts[index]]
    }
}

/// Parses the Initialize SPL Token Instruction and returns a TokenAccount
fn parse_token_account(instruction: &WrappedInstruction, context: &TransactionContext, owner: Option<Pubkey>) -> TokenAccount {
    let address = context.get_account_from_index(instruction.accounts()[0] as usize).clone();
    let mint = context.get_account_from_index(instruction.accounts()[1] as usize).clone();
    let owner = match owner {
        Some(pubkey) => pubkey.to_bytes().to_vec(),
        None => context.get_account_from_index(instruction.accounts()[2] as usize).clone(),
    };
    TokenAccount {
        address,
        mint,
        owner
    }
}

pub trait ConfirmedTransactionExt {
    fn signature(&self) -> &Vec<u8>;
}

impl ConfirmedTransactionExt for ConfirmedTransaction {
    fn signature(&self) -> &Vec<u8> {
        &self.transaction.as_ref().unwrap().signatures[0]
    }
}
