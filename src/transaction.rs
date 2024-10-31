use std::collections::HashMap;

use substreams_solana::pb::sf::solana::r#type::v1::ConfirmedTransaction;

use crate::pubkey::{Pubkey, PubkeyRef};
use crate::instruction::{WrappedInstruction, get_flattened_instructions};
use crate::spl_token::{TokenAccount, TokenInstruction, TOKEN_PROGRAM_ID, WRAPPED_SOL_MINT};

use anyhow::{anyhow, Error};

/// Context that can provide enough information to process an instruction
pub struct TransactionContext<'a> {
    pub accounts: Vec<PubkeyRef<'a>>,
    pub token_accounts: HashMap<PubkeyRef<'a>, TokenAccount<'a>>,
    pub signers: Vec<PubkeyRef<'a>>,
    pub signature: String,
}

impl<'a> TransactionContext<'a> {
    fn new(transaction: &'a ConfirmedTransaction) -> Self {
        let accounts = transaction.resolved_accounts().iter().map(|x| PubkeyRef { 0: x }).collect::<Vec<_>>();
        let signature = bs58::encode(transaction.transaction.as_ref().unwrap().signatures.get(0).unwrap()).into_string();
        let num_required_signatures = transaction.transaction.as_ref().unwrap().message.as_ref().unwrap().header.as_ref().unwrap().num_required_signatures;
        let signers = accounts[..num_required_signatures as usize].to_vec();

        Self {
            accounts,
            token_accounts: HashMap::new(),
            signers,
            signature,
        }
    }

    pub fn build(transaction: &'a ConfirmedTransaction) -> Result<Self, &'static str> {
        let mut context = Self::new(transaction);

        for token_balance in &transaction.meta.as_ref().unwrap().pre_token_balances {
            let address = context.accounts[token_balance.account_index as usize].clone();
            let balance = Some(token_balance.ui_token_amount.as_ref().unwrap().amount.parse::<u64>().expect("Failed to parse u64"));
            let token_account = TokenAccount {
                address: address.clone(),
                mint: Pubkey::try_from_string(&token_balance.mint).unwrap(),
                owner: Pubkey::try_from_string(&token_balance.owner).unwrap(),
                pre_balance: balance,
                post_balance: balance,
            };
            context.token_accounts.insert(address, token_account);
        }

        let instructions = get_flattened_instructions(transaction);
        for instruction in instructions {
            context.update_accounts(&instruction);
        }

        Ok(context)
    }

    pub fn update_accounts(&mut self, instruction: &WrappedInstruction) {
        if self.accounts[instruction.program_id_index() as usize] != TOKEN_PROGRAM_ID {
            return;
        }
        match TokenInstruction::unpack(&instruction.data()) {
            Ok(TokenInstruction::InitializeAccount) => {
                let token_account = parse_token_account_from_initialize_account_instruction(instruction, self, None);
                self.token_accounts.insert(token_account.address.clone(), token_account);
            }
            Ok(TokenInstruction::InitializeAccount2 { owner }) |
            Ok(TokenInstruction::InitializeAccount3 { owner }) => {
                let token_account = parse_token_account_from_initialize_account_instruction(instruction, self, Some(owner));
                self.token_accounts.insert(token_account.address.clone(), token_account);
            }
            _ => ()
        }
    }

    pub fn update_balance(&mut self, instruction: &WrappedInstruction) {
        for token_account in self.token_accounts.values_mut() {
            token_account.pre_balance = token_account.post_balance;
        }
        if self.accounts[instruction.program_id_index() as usize] != TOKEN_PROGRAM_ID {
            return;
        }
        match TokenInstruction::unpack(&instruction.data()) {
            // Insert token account
            Ok(TokenInstruction::InitializeAccount) => {
                let token_account = parse_token_account_from_initialize_account_instruction(instruction, self, None);
                self.token_accounts.insert(token_account.address.clone(), token_account);
            }
            Ok(TokenInstruction::InitializeAccount2 { owner }) |
            Ok(TokenInstruction::InitializeAccount3 { owner }) => {
                let token_account = parse_token_account_from_initialize_account_instruction(instruction, self, Some(owner));
                self.token_accounts.insert(token_account.address.clone(), token_account);
            },

            // Update token account balance
            Ok(TokenInstruction::Transfer { amount }) => {
                let source_address = self.accounts[instruction.accounts()[0] as usize];
                let destination_address = self.accounts[instruction.accounts()[1] as usize];

                let source_account = self.token_accounts.get_mut(&source_address).unwrap();
                source_account.post_balance = source_account.post_balance.map(|x| x - amount);

                let destination_account = self.token_accounts.get_mut(&destination_address).unwrap();
                destination_account.post_balance = destination_account.post_balance.map(|x| x + amount);
            },
            Ok(TokenInstruction::TransferChecked { amount, decimals: _ }) => {
                let source_address = self.accounts[instruction.accounts()[0] as usize];
                let destination_address = self.accounts[instruction.accounts()[2] as usize];

                let source_account = self.token_accounts.get_mut(&source_address).unwrap();
                source_account.post_balance = source_account.post_balance.map(|x| x - amount);

                let destination_account = self.token_accounts.get_mut(&destination_address).unwrap();
                destination_account.post_balance = destination_account.post_balance.map(|x| x + amount);
            },
            Ok(TokenInstruction::MintTo { amount }) => {
                let address = self.accounts[instruction.accounts()[1] as usize];
                let account = self.token_accounts.get_mut(&address).unwrap();
                account.post_balance = account.post_balance.map(|x| x + amount);
            },
            Ok(TokenInstruction::MintToChecked { amount, decimals: _ }) => {
                let address = self.accounts[instruction.accounts()[1] as usize];
                let account = self.token_accounts.get_mut(&address).unwrap();
                account.post_balance = account.post_balance.map(|x| x + amount);
            },
            Ok(TokenInstruction::Burn { amount }) => {
                let address = self.accounts[instruction.accounts()[0] as usize];
                let account = self.token_accounts.get_mut(&address).unwrap();
                account.post_balance = account.post_balance.map(|x| x - amount);
            },
            Ok(TokenInstruction::BurnChecked { amount, decimals: _ }) => {
                let address = self.accounts[instruction.accounts()[0] as usize];
                let account = self.token_accounts.get_mut(&address).unwrap();
                account.post_balance = account.post_balance.map(|x| x - amount);
            },
            Ok(TokenInstruction::SyncNative) => {
                let address = self.accounts[instruction.accounts()[0] as usize];
                let account = self.token_accounts.get_mut(&address).unwrap();
                account.post_balance = None;
            },
            Ok(TokenInstruction::CloseAccount) => {
                let address = self.accounts[instruction.accounts()[0] as usize];
                let account = self.token_accounts.get_mut(&address).unwrap();
                account.post_balance = Some(0);
            },
            _ => ()
        }
    }

    pub fn get_token_account(&self, address: &PubkeyRef<'a>) -> Option<&TokenAccount> {
        self.token_accounts.get(address)
    }
}

/// Parses the Initialize SPL Token Instruction and returns a TokenAccount
fn parse_token_account_from_initialize_account_instruction<'a>(instruction: &WrappedInstruction, context: &TransactionContext<'a>, owner: Option<Pubkey>) -> TokenAccount<'a> {
    let address = context.accounts[instruction.accounts()[0] as usize].clone();
    let mint = context.accounts[instruction.accounts()[1] as usize].to_pubkey().unwrap();
    let owner = match owner {
        Some(pubkey) => pubkey,
        None => context.accounts[instruction.accounts()[2] as usize].to_pubkey().unwrap(),
    };
    let balance = if mint != WRAPPED_SOL_MINT { Some(0) } else { None };

    TokenAccount {
        address,
        mint,
        owner,
        pre_balance: balance,
        post_balance: balance,
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

pub fn get_signers(transaction: &ConfirmedTransaction) -> Vec<String> {
    let accounts = transaction.resolved_accounts().iter().map(|x| PubkeyRef { 0: x }).collect::<Vec<_>>();
    let num_required_signatures = transaction.transaction.as_ref().unwrap().message.as_ref().unwrap().header.as_ref().unwrap().num_required_signatures;
    accounts[..num_required_signatures as usize].iter().map(|x| x.to_string()).collect()
}
