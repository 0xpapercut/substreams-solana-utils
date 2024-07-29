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
    pub inner_instructions: Vec<Self>,
    pub logs: Vec<&'a String>,
    pub parent_instruction: Option<&'a StructuredInstruction<'a>>,
}

impl<'a> StructuredInstruction<'a> {
    fn new(instruction: WrappedInstruction<'a>, inner_instructions: Vec<StructuredInstruction<'a>>) -> Self {
        Self {
            instruction,
            inner_instructions: inner_instructions,
            logs: Vec::new(),
            parent_instruction: None,
        }
    }
    pub fn program_id_index(&self) -> u32 { self.instruction.program_id_index() }
    pub fn accounts(&self) -> &Vec<u8> { self.instruction.accounts() }
    pub fn data(&self) -> &Vec<u8> { self.instruction.data() }
    pub fn stack_height(&self) -> Option<u32> { self.instruction.stack_height() }
}

pub(crate) fn structure_wrapped_instructions_with_logs<'a>(instructions: Vec<WrappedInstruction<'a>>, logs: &[String]) -> Vec<StructuredInstruction<'a>> {
    let mut structured_instructions: Vec<StructuredInstruction> = Vec::new();

    if instructions.len() == 0 {
        return Vec::new();
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

    structured_instructions
}

pub trait StructuredInstructions {
    fn flattened(&self) -> Vec<&StructuredInstruction>;
}

impl<'a> StructuredInstructions for Vec<StructuredInstruction<'a>> {
    fn flattened(&self) -> Vec<&StructuredInstruction> {
        let mut instructions: Vec<&StructuredInstruction> = Vec::new();
        for instruction in self {
            instructions.push(instruction);
            instructions.extend(instruction.inner_instructions.flattened());
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

pub fn get_structured_instructions(transaction: &pb::ConfirmedTransaction) -> Vec<StructuredInstruction> {
    let wrapped_instructions = get_wrapped_instructions(transaction);
    let logs = transaction.meta.as_ref().unwrap().log_messages.as_ref();
    structure_wrapped_instructions_with_logs(wrapped_instructions, logs)
}
