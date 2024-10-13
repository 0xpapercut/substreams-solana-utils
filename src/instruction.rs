use std::rc::{Rc, Weak};
use std::cell::{Ref, RefCell};
use std::iter::Peekable;
use substreams_solana::b58;
use substreams_solana::pb::sf::solana::r#type::v1 as pb;
use anyhow::{anyhow, Error};

use crate::log::Log;
use crate::pubkey::{Pubkey, PubkeyRef};

#[derive(Debug)]
pub enum WrappedInstruction<'a> {
    Compiled(&'a pb::CompiledInstruction),
    Inner(&'a pb::InnerInstruction),
}

impl WrappedInstruction<'_> {
    pub fn program_id_index(&self) -> u32 {
        match self {
            Self::Compiled(instruction) => instruction.program_id_index,
            Self::Inner(instruction) => instruction.program_id_index,
        }
    }
    pub fn accounts(&self) -> &Vec<u8> {
        match self {
            Self::Compiled(instruction) => &instruction.accounts,
            Self::Inner(instruction) => &instruction.accounts,
        }
    }
    pub fn data(&self) -> &Vec<u8> {
        match self {
            Self::Compiled(instruction) => &instruction.data,
            Self::Inner(instruction) => &instruction.data,
        }
    }
    pub fn stack_height(&self) -> Option<u32> {
        match self {
            Self::Compiled(_) => Some(1),
            Self::Inner(instruction) => instruction.stack_height,
        }
    }
}

impl<'a> From<&'a pb::CompiledInstruction> for WrappedInstruction<'a> {
    fn from(value: &'a pb::CompiledInstruction) -> Self {
        WrappedInstruction::Compiled(&value)
    }
}

impl<'a> From<&'a pb::InnerInstruction> for WrappedInstruction<'a> {
    fn from(value: &'a pb::InnerInstruction) -> Self {
        WrappedInstruction::Inner(&value)
    }
}

const PROGRAMS_WITHOUT_LOGGING: &[Pubkey] = &[
    Pubkey(b58!("Ed25519SigVerify111111111111111111111111111")),
    Pubkey(b58!("KeccakSecp256k11111111111111111111111111111")),
];

#[derive(Debug)]
pub struct StructuredInstruction<'a> {
    instruction: WrappedInstruction<'a>,
    accounts: Vec<PubkeyRef<'a>>,
    program_id: PubkeyRef<'a>,
    inner_instructions: RefCell<Vec<Rc<Self>>>,
    parent_instruction: RefCell<Option<Weak<Self>>>,
    logs: RefCell<Option<Vec<Log<'a>>>>,
}

impl<'a> StructuredInstruction<'a> {
    fn new(instruction: WrappedInstruction<'a>, inner_instructions: RefCell<Vec<Rc<Self>>>, accounts: &Vec<&'a Vec<u8>>) -> Self {
        let instruction_accounts: Vec<_> = instruction.accounts().iter().map(|i| PubkeyRef(accounts[*i as usize])).collect();
        let program_id = PubkeyRef(accounts[instruction.program_id_index() as usize]);
        Self {
            instruction,
            program_id,
            accounts: instruction_accounts,
            inner_instructions: inner_instructions,
            parent_instruction: RefCell::new(None),
            logs: RefCell::new(None),
        }
    }
    pub fn program_id(&self) -> PubkeyRef<'a> { self.program_id }
    pub fn program_id_index(&self) -> u32 { self.instruction.program_id_index() }
    pub fn accounts(&self) -> &Vec<PubkeyRef> { &self.accounts }
    pub fn data(&self) -> &Vec<u8> { self.instruction.data() }
    pub fn stack_height(&self) -> Option<u32> { self.instruction.stack_height() }
    pub fn inner_instructions(&self) -> Ref<Vec<Rc<Self>>> { self.inner_instructions.borrow() }
    pub fn parent_instruction(&self) -> Option<Rc<Self>> { self.parent_instruction.borrow().as_ref().map(|x| x.upgrade().unwrap()) }
    pub fn logs(&self) -> Ref<Option<Vec<Log<'a>>>> { self.logs.borrow() }

    pub fn top_instruction(&self) -> Option<Rc<Self>> {
        if let Some(instruction) = self.parent_instruction() {
            let mut top_instruction = instruction;
            while let Some(parent_instruction) = top_instruction.parent_instruction() {
                top_instruction = parent_instruction;
            }
            Some(top_instruction)
        } else {
            None
        }
    }
}

pub struct LogStack<'a> {
    stack: Vec<Vec<Log<'a>>>,
    is_truncated: bool,
}

impl<'a> LogStack<'a> {
    pub fn new() -> Self {
        Self { stack: Vec::new(), is_truncated: false }
    }

    pub fn open<I>(&mut self, logs: &mut Peekable<I>, program_id: PubkeyRef)
    where
        I: Iterator<Item = Log<'a>>
    {
        if PROGRAMS_WITHOUT_LOGGING.iter().any(|x| *x == program_id) || self.is_truncated {
            return;
        }
        loop {
            let log = logs.next().unwrap();

            if log.is_truncated() {
                self.is_truncated = true;
                break;
            } else if log.is_invoke() {
                self.stack.push(vec![log]);
                break;
            } else {
                self.stack.last_mut().unwrap().push(log);
            }
        }
    }

    pub fn close<I>(&mut self, logs: &mut Peekable<I>, program_id: PubkeyRef) -> Option<Vec<Log<'a>>>
    where
        I: Iterator<Item = Log<'a>>
    {
        if PROGRAMS_WITHOUT_LOGGING.iter().any(|x| *x == program_id) {
            return Some(Vec::new());
        }
        if self.is_truncated {
            return None;
        }

        loop {
            let log = logs.next().unwrap();

            if log.is_truncated() {
                self.is_truncated = true;
                return None;
            } else if log.is_invoke() {
                panic!("Unexpected invoke log");
            }

            let is_success = log.is_success();
            self.stack.last_mut().unwrap().push(log);
            if is_success {
                return self.stack.pop()
            }
        }
    }
}

pub fn structure_flattened_instructions_with_logs<'a, I>(
    flattened_instructions: Vec<WrappedInstruction<'a>>,
    logs: &mut Peekable<I>,
    accounts: Vec<&'a Vec<u8>>,
) -> Vec<Rc<StructuredInstruction<'a>>>
where
    I: Iterator<Item = Log<'a>>
{
    let mut structured_instructions: Vec<Rc<StructuredInstruction<'a>>> = Vec::new();
    let mut instruction_stack: Vec<Rc<StructuredInstruction<'a>>> = Vec::new();
    let mut log_stack = LogStack::new();

    for instruction in flattened_instructions {
        let structured_instruction = Rc::new(StructuredInstruction::new(instruction, Vec::new().into(), &accounts));

        while !instruction_stack.is_empty() && instruction_stack.last().unwrap().stack_height() >= structured_instruction.stack_height() {
            let popped_instruction = instruction_stack.pop().unwrap();
            *popped_instruction.logs.borrow_mut() = log_stack.close(logs, popped_instruction.program_id());

            if !instruction_stack.is_empty() {
                *popped_instruction.parent_instruction.borrow_mut() = Some(Rc::downgrade(instruction_stack.last().unwrap()));
                instruction_stack.last_mut().unwrap().inner_instructions.borrow_mut().push(popped_instruction);
            } else {
                structured_instructions.push(popped_instruction);
            }
        }

        log_stack.open(logs, structured_instruction.program_id());
        instruction_stack.push(structured_instruction);
    }

    while !instruction_stack.is_empty() {
        let popped_instruction = instruction_stack.pop().unwrap();
        *popped_instruction.logs.borrow_mut() = log_stack.close(logs, popped_instruction.program_id());

        if !instruction_stack.is_empty() {
            instruction_stack.last_mut().unwrap().inner_instructions.borrow_mut().push(popped_instruction);
        } else {
            structured_instructions.push(popped_instruction)
        }
    }

    structured_instructions
}

pub fn get_flattened_instructions(confirmed_transaction: &pb::ConfirmedTransaction) -> Vec<WrappedInstruction> {
    let compiled_instructions = confirmed_transaction.transaction.as_ref().map(|x| x.message.as_ref().map(|y| &y.instructions)).unwrap().unwrap();
    let inner_instructions = confirmed_transaction.meta.as_ref().map(|x| &x.inner_instructions).unwrap();

    let mut wrapped_instructions: Vec<WrappedInstruction> = Vec::new();
    let mut j = 0;
    for (i, instr) in compiled_instructions.iter().enumerate() {
        wrapped_instructions.push(instr.into());
        if let Some(inner) = inner_instructions.get(j) {
            if inner.index == i as u32 {
                wrapped_instructions.extend(inner_instructions[j].instructions.iter().map(|x| WrappedInstruction::from(x)));
                j += 1;
            }
        }
    }
    wrapped_instructions
}

pub fn get_structured_instructions<'a>(transaction: &'a pb::ConfirmedTransaction) -> Result<Vec<Rc<StructuredInstruction<'a>>>, Error> {
    if let Some(_) = transaction.meta.as_ref().unwrap().err {
        return Err(anyhow!("Cannot structure instructions of a failed transaction."));
    }
    let flattened_instructions: Vec<WrappedInstruction> = get_flattened_instructions(transaction);
    let logs: &Vec<_> = transaction.meta.as_ref().unwrap().log_messages.as_ref();
    let accounts = transaction.resolved_accounts();
    Ok(structure_flattened_instructions_with_logs(flattened_instructions, &mut logs.iter().map(|log| Log::new(log)).peekable(), accounts))
}

pub trait StructuredInstructions<'a> {
    fn flattened(&self) -> Vec<Rc<StructuredInstruction<'a>>>;
}

impl<'a> StructuredInstructions<'a> for Vec<Rc<StructuredInstruction<'a>>> {
    fn flattened(&self) -> Vec<Rc<StructuredInstruction<'a>>> {
        let mut instructions: Vec<Rc<StructuredInstruction>> = Vec::new();
        for instruction in self {
            instructions.push(Rc::clone(instruction));
            instructions.extend(instruction.inner_instructions().flattened().iter().map(Rc::clone));
        }
        instructions
    }
}
