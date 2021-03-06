use std::convert::{TryInto, TryFrom};
use std::{fmt, fs, io, ops};
use std::path::Path;
use std::io::{Write, BufRead};
use std::collections::VecDeque;
use std::cell::RefCell;
use std::rc::Rc;
use crate::emulator::Opcode::Halt;

pub type Word = i64;
pub type InputHandler = dyn FnMut(&mut Context) -> io::Result<Word>;
pub type OutputHandler = dyn FnMut(&mut Context, Word) -> io::Result<()>;

pub const MEMSIZE: usize = 1 << 15;  // 32 KiB

const MODE_POSITION: Word = 0;
const MODE_IMMEDIATE: Word = 1;
const MODE_RELATIVE: Word = 2;


/// An Intcode program
#[derive(Clone, Debug)]
pub struct Program(Vec<Word>);

impl Program {
    pub fn new(instructions: &[Word]) -> Program {
        Program(instructions.to_owned())
    }

    pub fn from_file<T: AsRef<Path>>(path: T) -> Result<Program, String> {
        let file = fs::File::open(&path).map_err(|err| format!("Failed to open file: {}", err))?;

        let mut reader = io::BufReader::new(file);
        let mut line = String::new();
        reader.read_line(&mut line).map_err(|err| format!("Failed to read line: {}", err))?;

        let instructions: Result<Vec<Word>, String> = line.trim()
            .split(',')
            .map(|val| val.parse::<Word>().map_err(|err| { format!("Failed to parse value {:?}: {}", val, err) }))
            .collect();

        Ok(Program::new(&instructions?))
    }
}

impl ops::Index<usize> for Program {
    type Output = Word;

    fn index(&self, index: usize) -> &Self::Output {
        &self.0[index]
    }
}

impl ops::IndexMut<usize> for Program {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.0[index]
    }
}

/// Emulates an Intcode computer
pub struct IntcodeEmulator {
    ip: usize,
    relbase: Word,
    mem: Vec<Word>,
    decoded_instruction: Instruction,
    input_handler: Box<InputHandler>,
    output_handler: Box<OutputHandler>,
    yield_: bool,
    debug: bool,
}

impl IntcodeEmulator {
    /// Create a new IntcodeEmulator
    pub fn new(input_handler: Box<InputHandler>, output_handler: Box<OutputHandler>) -> IntcodeEmulator {
        let decoded_instruction = Instruction::new(Halt.into()).unwrap();
        IntcodeEmulator {
            ip: 0,
            relbase: 0,
            mem: vec![decoded_instruction.into()],
            decoded_instruction,
            input_handler,
            output_handler,
            yield_: false,
            debug: false,
        }
    }

    /// The current instruction pointer address
    pub fn ip(&self) -> usize {
        self.ip
    }

    /// Set the current instruction pointer
    pub fn set_ip(&mut self, ip: usize) {
        self.ip = ip;
    }

    /// The current relative base
    pub fn rb(&self) -> Word {
        self.relbase
    }

    /// Set the current relative base
    pub fn set_rb(&mut self, rb: Word) {
        self.relbase = rb;
    }

    /// The current decoded instruction
    pub fn current_instruction(&self) -> Result<Instruction, Exception> {
        Instruction::new(*self.mem.get(self.ip).ok_or_else(|| Exception::SegmentationFault(self.ip))?)
    }

    /// Is the CPU halted
    pub fn is_halted(&self) -> bool {
        self.current_instruction().map(|x| x.op.is_halt())
            .unwrap_or(false)
    }

    /// The current memory contents
    pub fn mem(&self) -> &[Word] {
        &self.mem
    }

    /// The current memory contents
    pub fn mem_mut(&mut self) -> &mut [Word] {
        &mut self.mem
    }

    pub fn set_input_handler(&mut self, handler: Box<InputHandler>) {
        self.input_handler = handler;
    }

    pub fn set_output_handler(&mut self, handler: Box<OutputHandler>) {
        self.output_handler = handler;
    }

    /// Load a program into memory
    pub fn load_program(&mut self, program: &Program) {
        self.ip = 0;
        self.mem = vec![0; MEMSIZE];
        self.mem.splice(..program.0.len(), program.0.iter().copied());
    }

    /// Get debugging flag
    pub fn get_debug(&self) -> bool {
        self.debug
    }

    /// Set debugging flag
    pub fn set_debug(&mut self, debug: bool) {
        self.debug = debug;
    }

    /// Run a program until an exception is encountered
    pub fn run(&mut self) -> Result<(), Exception> {
        while !self.is_halted() {
            self.step()?
        }
        Ok(())
    }

    /// Try to step a single instruction
    pub fn step(&mut self) -> Result<(), Exception> {
        if self.ip >= self.mem.len() {
            return Err(Exception::SegmentationFault(self.ip));
        }

        self.decoded_instruction = self.current_instruction().map_err(|_| Exception::IllegalInstruction(self.mem[self.ip]))?;
        if self.debug {
            self.print_disassembled();
        }

        if self.ip + self.decoded_instruction.op.nparams() >= self.mem.len() {
            return Err(Exception::SegmentationFault(self.ip));
        }

        match self.decoded_instruction.op {
            Opcode::Add => {
                *self.store(3)? = self.load(1)? + self.load(2)?;
            },
            Opcode::Mul => {
                *self.store(3)? = self.load(1)? * self.load(2)?;
            },
            Opcode::Input => {
                let mut context = Context::new();
                *self.store(1)? = (self.input_handler)(&mut context).map_err(Exception::IOError)?;
                self.yield_ = context.yield_;
            },
            Opcode::Output => {
                let mut context = Context::new();
                let word = self.load(1)?;
                (self.output_handler)(&mut context, word).map_err(Exception::IOError)?;
                self.yield_ = context.yield_;
            },
            Opcode::JumpIfTrue => {
                if self.load(1)? != 0 {
                    self.ip = self.load(2)?.try_into()  // must not be negative
                        .or(Err(Exception::IllegalInstruction(self.mem[self.ip])))?;
                    return self.maybe_yield();
                }
            },
            Opcode::JumpIfFalse => {
                if self.load(1)? == 0 {
                    self.ip = self.load(2)?.try_into()  // must not be negative
                        .or(Err(Exception::IllegalInstruction(self.mem[self.ip])))?;
                    return self.maybe_yield();
                }
            },
            Opcode::LessThan => {
                *self.store(3)? = if self.load(1)? < self.load(2)? { 1 } else { 0 };
            },
            Opcode::Equal => {
                *self.store(3)? = if self.load(1)? == self.load(2)? { 1 } else { 0 };
            },
            Opcode::SetRBOffset => {
                self.relbase += self.load(1)?;
            }
            Opcode::Halt => return Ok(()),
        };
        self.ip += self.decoded_instruction.op.nparams() + 1;

        self.maybe_yield()
    }

    /// Check if the emulator should yield
    /// Resets `yield` flag
    fn maybe_yield(&mut self) -> Result<(), Exception> {
        if self.yield_ {
            self.yield_ = false;
            Err(Exception::Yield)
        } else {
            Ok(())
        }
    }

    /// Dump registers to console
    pub fn dump_registers(&self) {
        eprintln!("ip:0x{:08x} rb:{}", self.ip, self.relbase);
    }

    /// Dump memory to console
    pub fn dump_memory(&self) {
        eprintln!("Dumping memory...");
        for addr in (0..self.mem.len()).step_by(8) {
            let flag = if addr == (self.ip & (!0 - 0b111)) { '>' } else { ' ' };
            let mem = &self.mem[addr..self.mem.len().min(addr+8)];
            if mem.iter().all(|&v| v == 0) && flag == ' ' {
                // Don't print empty blocks of memory
                continue;
            }

            let line: Vec<_> = mem.iter().enumerate()
                .map(|(offset, &val)| {
                    let flag = if addr + offset == self.ip { '←' } else { ' ' };
                    format!("{:-11}{}", val, flag)
                }).collect();
            eprintln!("{} {:08x} {}", flag, addr, line.join(" "));
        }
    }

    /// Print the disassembled current instruction to the console
    pub fn print_disassembled(&self) {
        eprintln!("{:08x} {}", self.ip, self.disassemble().unwrap_or_else(|_| String::from("???")));
    }

    /// Disassemble the current instruction
    pub fn disassemble(&self) -> Result<String, String> {
        let instruction = self.current_instruction().map_err(|err| format!("Failed to decode instruction: {}", err))?;
        let params: Vec<_> = self.mem[self.ip+1..].iter()
            .chain([0].iter().cycle())
            .take(instruction.op().nparams())
            .enumerate()
            .map(|(n, &p)| (instruction.mode_for(n + 1), p))
            .collect();

        let params_str: Vec<_> = params.iter().map(|&(m, p)| {
            match m {
                MODE_POSITION => format!("0x{:08x}", p),
                MODE_IMMEDIATE => format!("${}", p),
                MODE_RELATIVE => format!("%rb{:+}", p),
                _ => format!("?{}", p),
            }
        }).collect();

        Ok(format!("{} {}", instruction.op(), params_str.join(" ")))
    }

    /// Load a value from memory
    fn load(&self, param: usize) -> Result<Word, Exception> {
        assert!(param >= 1);
        let mode = self.decoded_instruction.mode_for(param);
        let addr = self.ip + param;
        let value = self.mem.get(addr).copied().ok_or(Exception::SegmentationFault(addr))?;
        match mode {
            MODE_POSITION => {
                // Must not be negative
                let addr = value.try_into().or_else(|_| Err(Exception::IllegalInstruction(self.mem[self.ip])))?;
                self.mem.get(addr).copied().ok_or(Exception::SegmentationFault(addr))
            },
            MODE_IMMEDIATE => Ok(value),
            MODE_RELATIVE => {
                let addr = (self.relbase + value).try_into().or_else(|_| Err(Exception::IllegalInstruction(self.mem[self.ip])))?;
                self.mem.get(addr).copied().ok_or(Exception::SegmentationFault(addr))
            },
            _ => Err(Exception::IllegalInstruction(self.mem[self.ip]))
        }
    }

    /// Store a value to memory
    fn store(&mut self, param: usize) -> Result<&mut Word, Exception> {
        assert!(param >= 1);
        let mode = self.decoded_instruction.mode_for(param);
        let addr = self.ip + param;
        let value = self.mem.get(addr).copied().ok_or(Exception::SegmentationFault(addr))?;
        match mode {
            MODE_POSITION => {
                // Must not be negative
                let addr = value.try_into().or_else(|_| Err(Exception::IllegalInstruction(self.mem[self.ip])))?;
                self.mem.get_mut(addr).ok_or(Exception::SegmentationFault(addr))
            },
            MODE_RELATIVE => {
                let addr = (self.relbase + value).try_into().or_else(|_| Err(Exception::IllegalInstruction(self.mem[self.ip])))?;
                self.mem.get_mut(addr).ok_or(Exception::SegmentationFault(addr))
            },
            // NOTE: Immediate mode is invalid for store
            _ => Err(Exception::IllegalInstruction(self.mem[self.ip])),
        }
    }
}

impl Default for IntcodeEmulator {
    fn default() -> Self {
        IntcodeEmulator::new(Box::new(default_input_handler),
                             Box::new(default_output_handler))
    }
}

pub struct Context {
    yield_: bool,
}

impl Context {
    fn new() -> Self {
        Context { yield_: false }
    }

    /// Set the yield flag
    pub fn set_yield(&mut self, yield_: bool) {
        self.yield_ = yield_;
    }
}

pub fn default_input_handler(_: &mut Context) -> io::Result<Word> {
    let mut inbuf = String::new();
    io::stdin().read_line(&mut inbuf)?;
    let input = inbuf.trim().parse::<Word>()
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    Ok(input)
}

pub fn default_output_handler(_: &mut Context, word: i64) -> io::Result<()> {
    writeln!(&mut io::stdout(), "{}", word)
}

pub struct AsciiIOHandler {
    input_buffer: Rc<RefCell<VecDeque<Word>>>,
}

impl AsciiIOHandler {
    pub fn new() -> Self {
        AsciiIOHandler { input_buffer: Rc::new(RefCell::new(VecDeque::new())) }
    }

    pub fn input_handler(&mut self) -> Box<InputHandler> {
        let input_buffer = Rc::clone(&self.input_buffer);

        Box::new(move |_| {
            let mut input_buffer = input_buffer.borrow_mut();
            while input_buffer.is_empty() {
                let mut line = String::new();
                io::stdin().read_line(&mut line)?;
                if !line.starts_with('#') {
                    input_buffer.extend(line.chars().map(|c| c as Word));
                }
            }
            input_buffer.pop_front().ok_or_else(|| io::Error::new(io::ErrorKind::BrokenPipe, "No more input"))
        })
    }

    pub fn output_handler(&self) -> Box<OutputHandler> {
        Box::new(|_, word| {
            if (0x00..=0x7F).contains(&word) {
                let c = word as u8 as char;
                print!("{}", c);
            } else {
                eprintln!("WARN: Non-ASCII output: {}", word);
            }

            Ok(())
        })
    }
}

/// Instruction
#[derive(Copy, Clone)]
pub struct Instruction {
    op: Opcode,
    modes: Word,
}

impl Instruction {
    fn new(instruction: Word) -> Result<Instruction, Exception> {
        let op = (instruction % 100).try_into().map_err(|_| Exception::IllegalInstruction(instruction))?;  // Lower 2 digits
        let modes = instruction / 100;  // Upper digits

        Ok(Instruction { op, modes })
    }

    /// Opcode of instruction
    pub fn op(self) -> Opcode {
        self.op
    }

    /// Mode for parameter `n`
    pub fn mode_for(self, param: usize) -> Word {
        assert!(param >= 1);
        let exponent = param.checked_sub(1).unwrap() as u32;

        let base: Word = 10;  // Ensure correct type
        (self.modes / base.pow(exponent)) % 10
    }
}

impl From<Instruction> for Word {
    fn from(instruction: Instruction) -> Self {
        instruction.modes * 100 + Word::from(instruction.op)
    }
}

/// Opcodes
#[derive(Copy, Clone, Eq, PartialEq)]
pub enum Opcode {
    Add,  // 1: [p3] = [p1] + [p2]
    Mul,  // 2: [p3] = [p1] * [p2]
    Input,  // 3: [p1] = read(STDIN)
    Output,  // 4: write(STDOUT) = [p1]
    JumpIfTrue,  // 5: if [p1] != 0 { ip = [p2] }
    JumpIfFalse,  // 6: if [p1] == 0 { ip = [p2] }
    LessThan,  // 7: [p3] = if [p1] < [p2] { 1 } else { 0 }
    Equal,  // 8: [p3] = if [p1] == [p2] { 1 } else { 0 }
    SetRBOffset,  // 9: relbase += [p1]
    Halt,  // 99: ...but don't catch fire
}

impl Opcode {
    /// Number of parameters this opcode takes
    pub fn nparams(self) -> usize {
        use Opcode::*;
        match self {
            Add => 3,
            Mul => 3,
            Input => 1,
            Output => 1,
            JumpIfTrue => 2,
            JumpIfFalse => 2,
            LessThan => 3,
            Equal => 3,
            SetRBOffset => 1,
            Halt => 0,
        }
    }

    /// Is this a Halt opcode
    pub fn is_halt(self) -> bool {
        self == Opcode::Halt
    }
}

impl fmt::Display for Opcode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        use Opcode::*;
        let s = match self {
            Add => "ADD",
            Mul => "MUL",
            Input => "INPUT",
            Output => "OUTPUT",
            JumpIfTrue => "JMPTRUE",
            JumpIfFalse => "JMPFALSE",
            LessThan => "CMPLT",
            Equal => "CMPEQ",
            SetRBOffset => "RBOFFSET",
            Halt => "HALT",
        };

        f.write_str(s)
    }
}

impl TryFrom<Word> for Opcode {
    type Error = ();

    fn try_from(word: Word) -> Result<Self, Self::Error> {
        use Opcode::*;
        match word {
            1 => Ok(Add),
            2 => Ok(Mul),
            3 => Ok(Input),
            4 => Ok(Output),
            5 => Ok(JumpIfTrue),
            6 => Ok(JumpIfFalse),
            7 => Ok(LessThan),
            8 => Ok(Equal),
            9 => Ok(SetRBOffset),
            99 => Ok(Halt),
            _ => Err(()),
        }
    }
}

impl From<Opcode> for Word {
    fn from(op: Opcode) -> Self {
        use Opcode::*;
        match op {
            Add => 1,
            Mul => 2,
            Input => 3,
            Output => 4,
            JumpIfTrue => 5,
            JumpIfFalse => 6,
            LessThan => 7,
            Equal => 8,
            SetRBOffset => 9,
            Halt => 99,
        }
    }
}

/// Exception status
#[derive(Debug)]
pub enum Exception {
    Yield,
    IllegalInstruction(Word),
    SegmentationFault(usize),
    IOError(io::Error),
}

impl fmt::Display for Exception {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        use Exception::*;
        f.write_str(&match &self {
            Yield => String::from("Yield"),
            IllegalInstruction(word) => format!("Illegal instruction {}", word),
            SegmentationFault(addr) => format!("Segmentation fault at {:08x}", addr),
            IOError(error) => format!("IO error: {}", error),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::rc::Rc;
    use std::cell::RefCell;

    #[test]
    fn test_day2_part1() {
        let mut cpu = IntcodeEmulator::default();
        let program = Program::from_file("../day02/input.txt").expect("Failed to read input");
        cpu.load_program(&program);
        cpu.mem_mut()[1] = 12;
        cpu.mem_mut()[2] = 2;
        assert!(cpu.run().is_ok());
        assert!(cpu.is_halted());

        assert_eq!(cpu.mem()[0], 4714701);
    }

    #[test]
    fn test_day2_part2() {
        let mut cpu = IntcodeEmulator::default();
        let program = Program::from_file("../day02/input.txt").expect("Failed to read input");
        cpu.load_program(&program);
        cpu.mem_mut()[1] = 51;
        cpu.mem_mut()[2] = 21;
        assert!(cpu.run().is_ok());
        assert!(cpu.is_halted());

        assert_eq!(cpu.mem()[0], 19690720);
    }

    #[test]
    fn test_day5_part1() {
        let program = Program::from_file("../day05/input.txt").expect("Failed to read input");
        assert_run(&program, VecDeque::from(vec![1]), &[0, 0, 0, 0, 0, 0, 0, 0, 0, 12440243]);
    }

    #[test]
    fn test_day5_part2() {
        let program = Program::from_file("../day05/input.txt").expect("Failed to read input");
        assert_run(&program, VecDeque::from(vec![5]), &[15486302]);
    }

    #[test]
    fn test_day9_part1() {
        let program = Program::from_file("../day09/input.txt").expect("Failed to read input");
        assert_run(&program, VecDeque::from(vec![1]), &[3335138414]);
    }

    fn assert_run(program: &Program, input: VecDeque<Word>, expected_output: &[Word]) {
        let input = Rc::new(RefCell::new(input));
        let output = Rc::new(RefCell::new(Vec::new()));

        {
            let input = Rc::clone(&input);
            let input_handler = Box::new(move |_: &mut Context| {
                input.borrow_mut().pop_back()
                    .ok_or_else(|| io::Error::new(io::ErrorKind::BrokenPipe, "Input exhausted"))
            });

            let output = Rc::clone(&output);
            let output_handler = Box::new(move |_: &mut Context, word| {
                output.borrow_mut().push(word);

                Ok(())
            });

            let mut cpu = IntcodeEmulator::new(input_handler, output_handler);
            cpu.load_program(&program);

            assert!(cpu.run().is_ok());
            assert!(cpu.is_halted());
        }

        let output = Rc::try_unwrap(output).unwrap().into_inner();
        assert_eq!(output, expected_output);
    }
}
