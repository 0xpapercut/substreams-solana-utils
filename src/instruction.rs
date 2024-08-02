use substreams_solana::pb::sf::solana::r#type::v1 as pb;

use crate::log::Log;

#[derive(Debug, Clone, Copy)]
pub enum WrappedInstruction<'a> {
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
    pub inner_instructions: Vec<Self>,
    pub logs: Vec<Log<'a>>,
}

impl<'a> StructuredInstruction<'a> {
    fn new(instruction: WrappedInstruction<'a>, inner_instructions: Vec<Self>) -> Self {
        Self {
            instruction,
            inner_instructions: inner_instructions,
            logs: Vec::new(),
        }
    }
    pub fn program_id_index(&self) -> u32 { self.instruction.program_id_index() }
    pub fn accounts(&self) -> &Vec<u8> { self.instruction.accounts() }
    pub fn data(&self) -> &Vec<u8> { self.instruction.data() }
    pub fn stack_height(&self) -> Option<u32> { self.instruction.stack_height() }
}

fn take_until_success_or_next_invoke_log<'a, I>(logs: &mut I) -> Vec<Log<'a>>
where
    I: Iterator<Item = Log<'a>>
{
    let mut i = 0;
    let mut stop = false;
    logs.take_while(|log| {
        if stop || (i > 0 && log.is_invoke()) {
            return false;
        } else if log.is_success() {
            stop = true
        }
        i += 1;
        true
    }).collect()
}

pub fn structure_flattened_instructions_with_logs<'a, I>(flattened_instructions: Vec<WrappedInstruction<'a>>, logs: &mut I) -> Vec<StructuredInstruction<'a>>
where
    I: Iterator<Item = Log<'a>>
{
    let mut structured_instructions: Vec<StructuredInstruction> = Vec::new();
    let mut instruction_stack: Vec<StructuredInstruction> = Vec::new();

    for instruction in flattened_instructions {
        let mut structured_instruction = StructuredInstruction::new(instruction, Vec::new().into());
        structured_instruction.logs.extend(take_until_success_or_next_invoke_log(logs));

        while !instruction_stack.is_empty() && structured_instruction.stack_height() <= instruction_stack.last().unwrap().stack_height() {
            let popped_instruction = instruction_stack.pop().unwrap();
            if let Some(last_instruction) = instruction_stack.last_mut() {
                last_instruction.inner_instructions.push(popped_instruction);
                last_instruction.logs.extend(take_until_success_or_next_invoke_log(logs));
            } else {
                structured_instructions.push(popped_instruction);
            }
        }
        instruction_stack.push(structured_instruction);
    }

    while !instruction_stack.is_empty() {
        let popped_instruction = instruction_stack.pop().unwrap();
        if let Some(last_instruction) = instruction_stack.last_mut() {
            last_instruction.inner_instructions.push(popped_instruction);
            last_instruction.logs.extend(take_until_success_or_next_invoke_log(logs));
        } else {
            structured_instructions.push(popped_instruction);
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

pub fn get_structured_instructions<'a>(transaction: &'a pb::ConfirmedTransaction) -> Vec<StructuredInstruction<'a>> {
    let flattened_instructions: Vec<WrappedInstruction> = get_flattened_instructions(transaction);
    let logs: &Vec<_> = transaction.meta.as_ref().unwrap().log_messages.as_ref();
    structure_flattened_instructions_with_logs(flattened_instructions, &mut logs.iter().map(|log| Log::new(log)))
}
