use anyhow::Error;
use substreams_solana::pb::sf::solana::r#type::v1::Block;

use substreams_solana_utils as utils;
use utils::instruction::get_structured_instructions;
use utils::transaction::get_signature;
use utils::instruction::StructuredInstructions;

use crate::pb;
use crate::pb::instruction_logs::InstructionLogsOutput;

#[substreams::handlers::map]
pub fn instruction_logs(block: Block) -> Result<InstructionLogsOutput, Error> {
    let mut output = InstructionLogsOutput::default();

    for transaction in block.transactions_owned() {
        let mut instructions: Vec<pb::instruction_logs::Instruction> = Vec::new();

        for instruction in get_structured_instructions(&transaction)?.flattened() {
            let logs: Vec<String> = instruction.logs().iter().map(|log| log.to_string()).collect();
            let program_id = instruction.program_id().to_string();
            instructions.push(pb::instruction_logs::Instruction { logs, program_id });
        }

        let signature = get_signature(&transaction);
        output.transactions.push(pb::instruction_logs::Transaction {
            signature,
            instructions,
        });
    }

    Ok(output)
}
