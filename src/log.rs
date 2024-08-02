use regex;
use base64;

pub fn build_structured_logs(logs: &Vec<&String>) -> Vec<ProgramStructuredLogs> {
    let mut structured_logs : Vec<ProgramStructuredLogs> = Vec::new();
    let mut log_stack: Vec<ProgramStructuredLogs> = Vec::new();

    let typed_logs = logs.iter().map(|s| Log::parse_log(s));
    for log in typed_logs {
        match log {
            Log::Invoke(invoke) => {
                log_stack.push(ProgramStructuredLogs::new(invoke.program_id))
            },
            Log::Success(success) => {
                let success_log = log_stack.pop().unwrap();
                if let Some(last_log) = log_stack.last_mut() {
                    last_log.inner_logs.push(success_log);
                } else {
                    structured_logs.push(success_log);
                }
            },
            Log::Data(data) => {
                log_stack.last_mut().unwrap().data = Some(data.data);
            },
            Log::Return(return_) => {
                log_stack.last_mut().unwrap().return_data = Some(return_.data);
            },
            Log::Program(program) => {
                log_stack.last_mut().unwrap().program_logs.push(program);
            },
            Log::Unknown(unknown) => {
                log_stack.last_mut().unwrap().unknown_logs.push(unknown)
            }
        }
    }
    structured_logs
}

pub struct ProgramStructuredLogs {
    program_id: String,
    data: Option<Vec<u8>>,
    return_data: Option<Vec<u8>>,
    program_logs: Vec<ProgramLog>,
    unknown_logs: Vec<UnknownLog>,
    inner_logs: Vec<Self>,
}

impl ProgramStructuredLogs {
    pub fn new(program_id: String) -> Self {
        Self {
            program_id,
            data: None,
            return_data: None,
            program_logs: Vec::new(),
            unknown_logs: Vec::new(),
            inner_logs: Vec::new(),
        }
    }

    pub fn update(&mut self, log: Log) {
        match log {
            Log::Data(data) => {
                self.data = Some(data.data);
            },
            Log::Return(return_) => {
                self.return_data = Some(return_.data);
            },
            Log::Program(program) => {
                self.program_logs.push(program);
            },
            Log::Unknown(unknown) => {
                self.unknown_logs.push(unknown)
            },
            _ => unimplemented!()
        }
    }
}

#[derive(Debug)]
pub enum Log {
    Invoke(InvokeLog), // "Program {} invoke [{}]",
    Success(SuccessLog), // Program {} success
    Return(ReturnLog), // "Program return: {} {}"
    Data(DataLog), //  "Program data: {}"
    Program(ProgramLog), // "Program log: {}"
    Unknown(UnknownLog),
}

impl Log {
    pub fn parse_log(log: &String) -> Self {
        if let Ok(invoke_log) = InvokeLog::parse_log(log) {
            return Self::Invoke(invoke_log);
        }
        if let Ok(success_log) = SuccessLog::parse_log(log) {
            return Self::Success(success_log);
        }
        if let Ok(return_log) = ReturnLog::parse_log(log) {
            return Self::Return(return_log);
        }
        if let Ok(data_log) = DataLog::parse_log(log) {
            return Self::Data(data_log);
        }
        if let Ok(program_log) = ProgramLog::parse_log(log) {
            return Self::Program(program_log);
        }
        Self::Unknown(UnknownLog { log: log.clone() })
    }

    pub fn is_success(&self) -> bool {
        matches!(self, Self::Success(_))
    }
    pub fn is_invoke(&self) -> bool {
        matches!(self, Self::Invoke(_))
    }

    pub fn is_return(&self) -> bool {
        matches!(self, Self::Return(_))
    }

    pub fn is_data(&self) -> bool {
        matches!(self, Self::Data(_))
    }

    pub fn is_program(&self) -> bool {
        matches!(self, Self::Program(_))
    }

    pub fn is_unknown(&self) -> bool {
        matches!(self, Self::Unknown(_))
    }
}

#[derive(Debug)]
pub struct ProgramLog {
    pub message: String,
}

impl ProgramLog {
    fn parse_log(log: &String) -> Result<Self, String> {
        let re = regex::Regex::new(r"Program log: (.+)").unwrap();
        if let Some(captures) = re.captures(&log) {
            let message = captures.get(1).unwrap().as_str().to_string();
            Ok(Self { message })
        } else {
            Err("This log does not seem to be of type ProgramLog.".into())
        }
    }
}

#[derive(Debug)]
pub struct InvokeLog {
    pub program_id: String,
    pub invoke_depth: u32,
}

impl InvokeLog {
    fn parse_log(log: &String) -> Result<Self, String> {
        let re = regex::Regex::new(r"Program (.+) invoke \[(\d+)\]").unwrap();
        if let Some(captures) = re.captures(&log) {
            let program_id = captures.get(1).unwrap().as_str().to_string();
            let invoke_depth = captures.get(2).unwrap().as_str().parse::<u32>().unwrap();
            Ok(Self { program_id, invoke_depth })
        } else {
            Err("This log does not seem to be of type InvokeLog.".into())
        }
    }
}

#[derive(Debug)]
pub struct SuccessLog {
    pub program_id: String,
}

impl SuccessLog {
    fn parse_log(log: &String) -> Result<Self, String> {
        let re = regex::Regex::new(r"Program (.+) success").unwrap();
        if let Some(captures) = re.captures(&log) {
            let program_id = captures.get(1).unwrap().as_str().to_string();
            Ok(Self { program_id })
        } else {
            Err("This log does not seem to be of type SuccessLog.".into())
        }
    }
}

#[derive(Debug)]
pub struct ReturnLog {
    program_id: String,
    data: Vec<u8>,
}

impl ReturnLog {
    fn parse_log(log: &String) -> Result<Self, String> {
        let re = regex::Regex::new(r"Program return: (.+) (.+)").unwrap();
        if let Some(captures) = re.captures(&log) {
            let program_id = captures.get(1).unwrap().as_str().to_string();
            let encoded_data = captures.get(2).unwrap().as_str();
            Ok(Self {
                program_id,
                data: base64::decode(encoded_data).map_err(|_| String::from("Base64 decoding error."))?
            })
        } else {
            Err("This log does not seem to be of type ReturnLog.".into())
        }
    }
}

#[derive(Debug)]
pub struct DataLog {
    pub data: Vec<u8>,
}

impl DataLog {
    fn parse_log(log: &String) -> Result<Self, String> {
        let re = regex::Regex::new(r"Program data: (.+)").unwrap();
        if let Some(captures) = re.captures(&log) {
            let encoded_data = captures.get(1).unwrap().as_str();
            Ok(Self {
                data: base64::decode(encoded_data).map_err(|_| String::from("Base64 decoding error."))?
            })
        } else {
            Err("This log does not seem to be of type DataLog.".into())
        }
    }
}

#[derive(Debug)]
pub struct UnknownLog {
    log: String,
}
