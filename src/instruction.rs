use std::cell::RefCell;
use std::rc::{Rc, Weak};

use substreams_solana::pb::sf::solana::r#type::v1 as pb;

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
    pub logs: Vec<&'a String>,
    pub parent_instruction: RefCell<Option<Weak<Self>>>,
}

impl<'a> StructuredInstruction<'a> {
    fn new(instruction: WrappedInstruction<'a>, inner_instructions: RefCell<Vec<Rc<Self>>>) -> Self {
        Self {
            instruction,
            inner_instructions: inner_instructions,
            logs: Vec::new(),
            parent_instruction: RefCell::new(None),
        }
    }
    pub fn program_id_index(&self) -> u32 { self.instruction.program_id_index() }
    pub fn accounts(&self) -> &Vec<u8> { self.instruction.accounts() }
    pub fn data(&self) -> &Vec<u8> { self.instruction.data() }
    pub fn stack_height(&self) -> Option<u32> { self.instruction.stack_height() }
}

pub(crate) fn structure_wrapped_instructions_with_logs<'a>(instructions: Vec<WrappedInstruction<'a>>, logs: &[String]) -> RefCell<Vec<Rc<StructuredInstruction<'a>>>> {
    let mut structured_instructions: Vec<StructuredInstruction> = Vec::new();

    if instructions.len() == 0 {
        return RefCell::new(Vec::new());
    }

    let stack_height = instructions[0].stack_height();
    let mut i = 0;
    for (j, instr) in instructions.iter().enumerate() {
        if j == i {
            continue;
        }
        if instr.stack_height() == stack_height {
            let inner_instructions = structure_wrapped_instructions_with_logs(instructions[i + 1..j].to_vec(), logs);
            structured_instructions.push(StructuredInstruction::new(instructions[i], inner_instructions));
            i = j;
        }
    }
    let inner_instructions = structure_wrapped_instructions_with_logs(instructions[i + 1..].to_vec(), logs);
    structured_instructions.push(StructuredInstruction::new(instructions[i], inner_instructions));
    let structured_instructions: RefCell<Vec<Rc<StructuredInstruction<'a>>>> = RefCell::new(structured_instructions.into_iter().map(Rc::new).collect());

    for parent_instruction in structured_instructions.borrow().iter() {
        for inner_instruction in parent_instruction.inner_instructions.borrow_mut().iter_mut() {
            *inner_instruction.parent_instruction.borrow_mut() = Some(Rc::downgrade(parent_instruction));
        }
    }

    structured_instructions
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

fn get_wrapped_instructions(confirmed_transaction: &pb::ConfirmedTransaction) -> Vec<WrappedInstruction> {
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
    let wrapped_instructions = get_wrapped_instructions(transaction);
    let logs = transaction.meta.as_ref().unwrap().log_messages.as_ref();
    structure_wrapped_instructions_with_logs(wrapped_instructions, logs)
}
