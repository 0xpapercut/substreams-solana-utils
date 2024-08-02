use std::borrow::BorrowMut;
use std::cell::RefCell;
use std::iter::Peekable;
use std::rc::{Rc, Weak};

use substreams::log::{info, println};
use substreams_solana::pb::sf::solana::r#type::v1 as pb;

use crate::log::Log;

#[derive(Debug, Clone, Copy)]
pub(crate) enum WrappedInstruction<'a> {
    Compiled(&'a pb::CompiledInstruction),
    Inner(&'a pb::InnerInstruction),
}

impl WrappedInstruction<'_> {
    pub fn program_id_index(&self) -> u32 {
        match self {
            Self::Compiled(instr) => instr.program_id_index,
            Self::Inner(instr) => instr.program_id_index,
        }
    }
    pub fn accounts(&self) -> &Vec<u8> {
        match self {
            Self::Compiled(instr) => &instr.accounts,
            Self::Inner(instr) => &instr.accounts,
        }
    }
    pub fn data(&self) -> &Vec<u8> {
        match self {
            Self::Compiled(instr) => &instr.data,
            Self::Inner(instr) => &instr.data,
        }
    }
    pub fn stack_height(&self) -> Option<u32> {
        match self {
            Self::Compiled(_) => Some(1),
            Self::Inner(instr) => instr.stack_height,
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

#[derive(Debug)]
pub struct StructuredInstruction<'a> {
    instruction: WrappedInstruction<'a>,
    pub inner_instructions: RefCell<Vec<Rc<Self>>>,
    pub logs: RefCell<Vec<Log>>,
    pub parent_instruction: RefCell<Option<Weak<Self>>>,
}

impl<'a> StructuredInstruction<'a> {
    fn new(instruction: WrappedInstruction<'a>, inner_instructions: RefCell<Vec<Rc<Self>>>) -> Self {
        Self {
            instruction,
            inner_instructions: inner_instructions,
            logs: RefCell::new(Vec::new()),
            parent_instruction: RefCell::new(None),
        }
    }
    pub fn program_id_index(&self) -> u32 { self.instruction.program_id_index() }
    pub fn accounts(&self) -> &Vec<u8> { self.instruction.accounts() }
    pub fn data(&self) -> &Vec<u8> { self.instruction.data() }
    pub fn stack_height(&self) -> Option<u32> { self.instruction.stack_height() }
}

fn take_until_success_or_next_invoke_log<I>(logs: &mut Peekable<I>) -> Vec<Log>
where
    I: Iterator<Item = Log>
{
    let mut taken_logs = Vec::new();
    let mut i = 0;
    while let Some(log) = logs.peek() {
        if log.is_invoke() && i > 0 {
            break;
        }
        if let Some(log) = logs.next() {
            let log_is_success = log.is_success();
            taken_logs.push(log);
            if log_is_success {
                break;
            }
        }
        i += 1;
    }
    taken_logs
}

pub fn structure_flattened_instructions_with_logs<'a>(flattened_instructions: Vec<WrappedInstruction<'a>>, logs: Peekable<impl Iterator<Item = Log>>) -> RefCell<Vec<Rc<StructuredInstruction<'a>>>> {
    let mut structured_instructions: Vec<Rc<StructuredInstruction>> = Vec::new();
    let mut instruction_stack: Vec<Rc<StructuredInstruction>> = Vec::new();
    let mut logs_iter = logs.into_iter().peekable();

    let mut i = 0;

    for instruction in flattened_instructions {
        let structured_instruction = StructuredInstruction::new(instruction, Vec::new().into());
        structured_instruction.logs.borrow_mut().extend(take_until_success_or_next_invoke_log(&mut logs_iter));

        while !instruction_stack.is_empty() && structured_instruction.stack_height() <= instruction_stack.last().unwrap().stack_height() {
            let popped_instruction = instruction_stack.pop().unwrap();
            if let Some(last_instruction) = instruction_stack.last() {
                *popped_instruction.parent_instruction.borrow_mut() = Some(Rc::downgrade(last_instruction));
                last_instruction.inner_instructions.borrow_mut().push(popped_instruction);
                last_instruction.logs.borrow_mut().extend(take_until_success_or_next_invoke_log(&mut logs_iter));
            } else {
                structured_instructions.push(popped_instruction);
            }
            i += 1;
            println(format!("{}", i));
        }
        instruction_stack.push(Rc::new(structured_instruction));
    }

    while !instruction_stack.is_empty() {
        let popped_instruction = instruction_stack.pop().unwrap();
        if let Some(last_instruction) = instruction_stack.last() {
            *popped_instruction.parent_instruction.borrow_mut() = Some(Rc::downgrade(last_instruction));
            last_instruction.inner_instructions.borrow_mut().push(popped_instruction);
            last_instruction.logs.borrow_mut().extend(take_until_success_or_next_invoke_log(&mut logs_iter));
        } else {
            structured_instructions.push(popped_instruction);
        }
    }

    RefCell::new(structured_instructions)
}

pub trait StructuredInstructions<'a> {
    fn flattened(&self) -> Vec<Rc<StructuredInstruction<'a>>>;
}

impl<'a> StructuredInstructions<'a> for Vec<Rc<StructuredInstruction<'a>>> {
    fn flattened(&self) -> Vec<Rc<StructuredInstruction<'a>>> {
        let mut instructions: Vec<Rc<StructuredInstruction>> = Vec::new();
        for instruction in self {
            instructions.push(Rc::clone(instruction));
            instructions.extend(instruction.inner_instructions.borrow().flattened());
        }
        instructions
    }
}

fn get_flattened_instructions(confirmed_transaction: &pb::ConfirmedTransaction) -> Vec<WrappedInstruction> {
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

pub fn get_structured_instructions<'a>(transaction: &'a pb::ConfirmedTransaction) -> RefCell<Vec<Rc<StructuredInstruction<'a>>>> {
    let flattened_instructions: Vec<WrappedInstruction> = get_flattened_instructions(transaction);
    let logs: &Vec<_> = transaction.meta.as_ref().unwrap().log_messages.as_ref();
    structure_flattened_instructions_with_logs(flattened_instructions, logs.iter().map(|log| Log::parse_log(log)).peekable())
}
