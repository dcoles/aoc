#![allow(clippy::unreadable_literal)]

use std::fs;
use std::path::Path;
use std::collections::VecDeque;
use std::convert::TryInto;

type Word = i32;

const MODE_POSITION: Word = 0;
const MODE_IMMEDIATE: Word = 1;

const OP_ADD: Word = 1;  // [p3] = [p1] + [p2]
const OP_MUL: Word = 2;  // [p3] = [p1] * [p2]
const OP_INPUT: Word = 3;  // [p1] = read(STDIN)
const OP_OUTPUT: Word = 4;  // write(STDOUT) = [p1]
const OP_JUMP_IF_TRUE: Word = 5;  // if [p1] != 0 { ip = [p2] }
const OP_JUMP_IF_FALSE: Word = 6; // if [p1] == 0 { ip = [p2] }
const OP_LT: Word = 7;  // [p3] = if [p1] < [p2] { 1 } else { 0 }
const OP_EQ: Word = 8;  // [p3] = if [p1] == [p2] { 1 } else { 0 }
const OP_HALT: Word = 99;  // ...but don't catch fire

const DEBUG: bool = false;

struct Program(Vec<Word>);

impl Program {
    fn new(instructions: &[Word]) -> Program {
        Program(instructions.to_owned())
    }
}

fn main() {
    let input = read_input("input.txt");

    // Part 1
    assert_eq!(43210,
               run_pipeline(&[4, 3, 2, 1, 0],
                            &Program::new(&[3, 15, 3, 16, 1002, 16, 10, 16, 1, 16, 15, 15, 4, 15, 99, 0, 0]),
                            false));
    assert_eq!(54321,
               run_pipeline(&[0, 1, 2, 3, 4],
                            &Program::new(&[3, 23, 3, 24, 1002, 24, 10, 24, 1002, 23, -1, 23, 101, 5, 23, 23, 1, 24, 23, 23, 4, 23, 99, 0, 0]),
                            false));
    assert_eq!(65210,
               run_pipeline(&[1, 0, 4, 3, 2],
                            &Program::new(&[3, 31, 3, 32, 1002, 32, 10, 32, 1001, 31, -2, 31, 1007, 31, 0, 33, 1002, 33, 7, 33, 1, 33, 31, 31, 1, 32, 31, 31, 4, 31, 99, 0, 0, 0]),
                            false));


    let (max_thrust, phase) = find_max(&[0,1,2,3,4], &input, false);
    println!("Part 1: Max thrust is {} ({:?})", max_thrust, phase);

    // Part 2
    assert_eq!(139629729,
               find_max(&[9,8,7,6,5],
                        &Program::new(&[3,26,1001,26,-4,26,3,27,1002,27,2,27,1,27,26,27,4,27,1001,28,-1,28,1005,28,6,99,0,0,5]),
                        true).0);

    assert_eq!(18216,
               find_max(&[9,8,7,6,5],
                        &Program::new(&[3,52,1001,52,-5,52,3,53,1,52,56,54,1007,54,5,55,1005,55,26,1001,54,-5,54,1105,1,12,1,53,54,53,1008,54,0,55,1001,55,1,55,2,53,55,53,4,53,1001,56,-1,56,1005,56,6,99,0,0,0,0,10]),
                        true).0);

    let (max_thrust, phase) = find_max(&[5,6,7,8,9], &input, true);
    println!("Part 2: Max thrust is {} ({:?})", max_thrust, phase);
}

fn read_input<T: AsRef<Path>>(path: T) -> Program {
    let contents = fs::read_to_string(path).expect("Failed to read input");
    let instructions = contents.trim().split(',').map(|line| line.parse::<Word>().expect("Failed to parse input")).collect();

    Program(instructions)
}

/// Find the permutation of phases that gives the maximum thrust
fn find_max(phases: &[Word], program: &Program, feedback: bool) -> (Word, Vec<Word>) {
    let mut max_thrust = 0;
    let mut phase = Vec::new();
    for perm in permutations(phases) {
        let thrust = run_pipeline(&perm, program, feedback);
        if thrust > max_thrust {
            max_thrust = thrust;
            phase = perm;
        }
    }

    (max_thrust, phase)
}

/// Run a pipeline of amplifiers
fn run_pipeline(phases: &[Word], program: &Program, feedback: bool) -> Word {
    // Set up amplifiers
    let mut amplifiers = Vec::new();
    for &phase in phases {
        let mut amp = IntcodeEmulator::new();
        amp.load_program(&program);
        amp.add_input(phase);
        amplifiers.push(amp);
    }

    // Feed initial input into first amp
    amplifiers[0].add_input(0);

    // Queue of amps to run
    let mut runqueue = VecDeque::new();
    runqueue.push_back(0);  // Schedule first amp

    // Drive the pipeline until it halts
    let mut output = 0;
    while let Some(i) = runqueue.pop_front() {
        match amplifiers[i].run() {
            Exception::Halt => (),
            Exception::Input => {
                // Schedule upstream amp to get more input
                runqueue.push_back((i - 1) % amplifiers.len());
            },
            Exception::Output(out) => {
                if i == amplifiers.len() - 1 {
                    // Last amp outputs to thrusters
                    output = out;
                    if feedback {
                        // Feedback into first amplifier
                        amplifiers[0].add_input(out);
                    }
                } else {
                    // Feed into next amplifier
                    amplifiers[i + 1].add_input(out);
                }

                // Schedule downstream amp as it can now make progress
                runqueue.push_back((i + 1) % amplifiers.len())
            },
            Exception::IllegalInstruction(word) => panic!("Illegal instruction {}", word),
            Exception::SegmentationFault(word) => panic!("Segmentation fault at {:08x}", word),
        }
    }

    output
}

/// Calculate all permutations of a slice
fn permutations(input: &[Word]) -> Vec<Vec<Word>> {
    let mut input = input.to_owned();
    let len = input.len();

    fn permutations_(input: &mut [Word], k: usize) -> Vec<Vec<Word>> {
        if k == 1 {
            return vec![input.to_vec()];
        }

        let mut output = permutations_(input, k - 1);
        for i in 0..k-1 {
            if k % 2 == 0 {
                input.swap(i, k - 1);
            } else {
                input.swap(0, k - 1)
            }
            let mut perms = permutations_(input, k - 1);
            output.append(&mut perms);
        }

        output
    }

    permutations_(&mut input, len)
}

/// Emulates an Intcode computer
struct IntcodeEmulator {
    ip: usize,
    mem: Vec<Word>,
    input: VecDeque<Word>,
}

impl IntcodeEmulator {
    /// Create a new IntcodeEmulator
    fn new() -> IntcodeEmulator {
        IntcodeEmulator { ip: 0, mem: vec![OP_HALT], input: VecDeque::new() }
    }

    /// Load a program into memory
    fn load_program(&mut self, program: &Program) {
        self.ip = 0;
        self.mem = program.0.to_owned();
    }

    /// Queue input
    fn add_input(&mut self, input: Word) {
        self.input.push_back(input);
    }

    /// Run a program until an exception is encountered
    fn run(&mut self) -> Exception {
        loop {
            if let Err(exception) = self.step() {
                return exception;
            }
        }
    }

    /// Try to step a single instruction
    fn step(&mut self) -> Result<(), Exception> {
        if self.ip >= self.mem.len() {
            return Err(Exception::SegmentationFault(self.ip));
        }

        let op = self.op();
        if DEBUG {
            println!("{:08x} {}", self.ip, IntcodeEmulator::opcode_to_str(op));
        }
        match op {
            OP_ADD => {
                *self.store(3)? = self.load(1)? + self.load(2)?;
                self.ip += 4;
            },
            OP_MUL => {
                *self.store(3)? = self.load(1)? * self.load(2)?;
                self.ip += 4;
            },
            OP_INPUT => {
                if let Some(input) = self.input.pop_front() {
                    *self.store(1)? = input;
                    self.ip += 2;
                } else {
                    // Upcall to request input
                    return Err(Exception::Input);
                }
            },
            OP_OUTPUT => {
                let output = self.load(1)?;
                self.ip += 2;
                // Upcall for output
                return Err(Exception::Output(output));
            },
            OP_JUMP_IF_TRUE => {
                if self.load(1)? != 0 {
                    self.ip = self.load(2)?.try_into()  // must not be negative
                        .or(Err(Exception::IllegalInstruction(op)))?;
                } else {
                    self.ip += 3;
                }
            },
            OP_JUMP_IF_FALSE => {
                if self.load(1)? == 0 {
                    self.ip = self.load(2)?.try_into()  // must not be negative
                        .or(Err(Exception::IllegalInstruction(op)))?;
                } else {
                    self.ip += 3;
                }
            },
            OP_LT => {
                *self.store(3)? = if self.load(1)? < self.load(2)? { 1 } else { 0 };
                self.ip += 4;
            },
            OP_EQ => {
                *self.store(3)? = if self.load(1)? == self.load(2)? { 1 } else { 0 };
                self.ip += 4;
            },
            OP_HALT => return Err(Exception::Halt),
            _ => return Err(Exception::IllegalInstruction(op)),
        };

        Ok(())
    }

    /// The current instruction's op-code
    fn op(&self) -> Word {
        self.mem[self.ip] % 100
    }

    /// The current instruction's parameter modes
    fn modes(&self) -> Word {
        self.mem[self.ip] / 100
    }

    /// Load a value from memory
    fn load(&self, param: usize) -> Result<Word, Exception> {
        let mode = self.mode(param)?;
        let addr = self.ip + param;
        let value = self.mem.get(addr).copied().ok_or(Exception::SegmentationFault(addr))?;
        match mode {
            MODE_POSITION => {
                // Must not be negative
                let addr = value.try_into().or_else(|_| Err(Exception::IllegalInstruction(self.op())))?;
                self.mem.get(addr).copied().ok_or(Exception::SegmentationFault(addr))
            },
            MODE_IMMEDIATE => Ok(value),
            _ => Err(Exception::IllegalInstruction(self.op()))
        }
    }

    /// Store a value to memory
    fn store(&mut self, param: usize) -> Result<&mut Word, Exception> {
        let mode = self.mode(param)?;
        let addr = self.ip + param;
        let value = self.mem.get(addr).copied().ok_or(Exception::SegmentationFault(addr))?;
        match mode {
            MODE_POSITION => {
                // Must not be negative
                let addr = value.try_into().or_else(|_| Err(Exception::IllegalInstruction(self.op())))?;
                self.mem.get_mut(addr).ok_or(Exception::SegmentationFault(addr))
            },
            MODE_IMMEDIATE => {
                // Illegal store in immediate mode
                Err(Exception::IllegalInstruction(self.op()))
            },
            _ => Err(Exception::IllegalInstruction(self.op())),
        }
    }

    /// Mode for parameter
    #[allow(clippy::identity_conversion)]
    fn mode(&self, param: usize) -> Result<Word, Exception> {
        if param == 0 {
            // Can't have a 0-th parameter
            return Err(Exception::IllegalInstruction(self.op()));
        }
        let exponent = param.checked_sub(1).unwrap() as u32;

        Ok(self.modes() / Word::from(10).pow(exponent) % 10)
    }

    /// Return the mnemonic for an opcode
    fn opcode_to_str(opcode: Word) -> &'static str {
        match opcode {
            OP_ADD => "ADD",
            OP_MUL => "MUL",
            OP_INPUT => "INPUT",
            OP_OUTPUT => "OUTPUT",
            OP_JUMP_IF_TRUE => "JMPTRUE",
            OP_JUMP_IF_FALSE => "JMPFALSE",
            OP_LT => "CMPLT",
            OP_EQ => "CMPEQ",
            OP_HALT => "HALT",
            _ => "UNKNOWN",
        }
    }
}

/// Exception status
enum Exception {
    Halt,
    IllegalInstruction(Word),
    SegmentationFault(usize),
    Input,
    Output(Word),
}
