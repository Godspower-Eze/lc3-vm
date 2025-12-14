use std::{env, fs::File, io::{self, Read, Write}, usize, mem};
use libc::{termios, tcgetattr, tcsetattr, ICANON, ECHO, TCSANOW, fd_set, timeval, FD_SET, FD_ZERO, select};

const MEMORY_SIZE: usize = 1 << 16;

static mut ORIGINAL_TERMIOS: Option<termios> = None;

static mut KEY_READY: bool = false;
static mut KEY_VALUE: u16 = 0;

const PC_START: u16 = 0x3000; /* default starting position for the program counter */


enum REGISTER {
    R0,
    R1,
    R2,
    R3,
    R4,
    R5,
    R6,
    R7,
    PC, /* program counter */
    COND,
    COUNT
}

#[derive(Debug)]
enum InstructionSet
{
    BR, /* branch */
    ADD,    /* add  */
    LD,     /* load */
    ST,     /* store */
    JSR,    /* jump register */
    AND,    /* bitwise and */
    LDR,    /* load register */
    STR,    /* store register */
    RTI,    /* unused */
    NOT,    /* bitwise not */
    LDI,    /* load indirect */
    STI,    /* store indirect */
    JMP,    /* jump */
    RES,    /* reserved (unused) */
    LEA,    /* load effective address */
    TRAP    /* execute trap */
}

#[derive(Debug)]
enum ConditionFlags
{
    POS = 1 << 0, /* P */
    ZRO = 1 << 1, /* Z */
    NEG = 1 << 2, /* N */
}

#[derive(Debug)]
enum TrapCodes {
    GETC = 0x20,  /* get character from keyboard, not echoed onto the terminal */
    OUT = 0x21,   /* output a character */
    PUTS = 0x22,  /* output a word string */
    IN = 0x23,    /* get character from keyboard, echoed onto the terminal */
    PUTSP = 0x24, /* output a byte string */
    HALT = 0x25   /* halt the program */
}

enum MemoryMappedRegisters {
    KBSR = 0xFE00, /* keyboard status */
    KBDR = 0xFE02  /* keyboard data */
}

fn sign_extend(value: u16, bit_count: u8) -> u16 {
    let result = if (value >> (bit_count - 1)) & 0x1 == 1 {
        value | (0xFFFF << bit_count)
    } else {
        value
    };
    result
}

fn update_flags(addr: u16, registers: &mut [u16]) {
    let value = registers[addr as usize];
    if value == 0 {
        registers[REGISTER::COND as usize] = ConditionFlags::ZRO as u16;
    } else if (value >> 15) == 1 {
        registers[REGISTER::COND as usize] = ConditionFlags::NEG as u16;
    } else {
        registers[REGISTER::COND as usize] = ConditionFlags::POS as u16;
    }
}

fn get_instructions(file_path: &str) -> io::Result<Vec<u16>> {
    let mut file = File::open(file_path)?;
    
    let mut buf = Vec::new();
    file.read_to_end(&mut buf)?;

    // Must be an even number of bytes
    assert!(buf.len() % 2 == 0);

    let mut words = Vec::new();
    for chunk in buf.chunks_exact(2) {
        let word = u16::from_be_bytes([chunk[0], chunk[1]]);
        words.push(word);
    }
    return Ok(words);
}

fn load_memory(instructions: Vec<u16>) -> [u16; MEMORY_SIZE] {
    let mut memory: [u16; MEMORY_SIZE] = [0; MEMORY_SIZE];
    let origin = instructions[0];
    let modified_instruction = &instructions[1..];
    for (i, instruction) in modified_instruction.iter().enumerate() {
        memory[(origin as usize + i) as usize] = *instruction;
    }
    return memory;
}

fn initialize_registers(origin: u16) -> [u16; REGISTER::COUNT as usize] {
    let mut registers: [u16; REGISTER::COUNT as usize] = [0; REGISTER::COUNT as usize];
    /* since exactly one condition flag should be set at any given time, set the Z flag */
    registers[REGISTER::COND as usize] = ConditionFlags::ZRO as u16;
    /* set the PC to starting position */
    registers[REGISTER::PC as usize] = origin;
    return registers;
}

pub fn disable_input_buffering() {
    unsafe {
        let mut t = mem::zeroed::<termios>();
        tcgetattr(0, &mut t);
        ORIGINAL_TERMIOS = Some(t);

        t.c_lflag &= !(ICANON | ECHO);
        tcsetattr(0, TCSANOW, &t);
    }
}

pub fn restore_input_buffering() {
    unsafe {
        if let Some(t) = ORIGINAL_TERMIOS {
            tcsetattr(0, TCSANOW, &t);
        }
    }
}

fn write_to_memory(memory: &mut [u16], address: u16, value: u16) {
    memory[address as usize] = value;
}

pub fn check_key() -> bool {
    unsafe {
        let mut readfds = std::mem::zeroed::<fd_set>();
        FD_ZERO(&mut readfds);
        FD_SET(0, &mut readfds); // stdin

        let mut timeout = timeval {
            tv_sec: 0,
            tv_usec: 0,
        };

        select(
            1,
            &mut readfds,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut timeout,
        ) > 0
    }
}

pub fn get_char() -> u8 {
    use std::io::Read;
    let mut buf = [0u8; 1];
    std::io::stdin().read_exact(&mut buf).unwrap();
    buf[0]
}


fn read_from_memory(memory: &mut [u16], address: u16) -> u16 {
    unsafe {
        if address == MemoryMappedRegisters::KBSR as u16 {
            if !KEY_READY && check_key() {
                KEY_VALUE = get_char() as u16;
                KEY_READY = true;
            }
            return if KEY_READY { 1 << 15 } else { 0 };
        }

        if address == MemoryMappedRegisters::KBDR as u16 {
            KEY_READY = false; // clear latch
            return KEY_VALUE;
        }
    }

    memory[address as usize]
}

fn run_program(memory: &mut [u16], registers: &mut [u16], tracing: &mut Vec<InstructionSet>) {
    let mut running = true;

    while running {
        let pc = registers[REGISTER::PC as usize];
        let instruction = read_from_memory(memory, pc);
        registers[REGISTER::PC as usize] = pc.wrapping_add(1);

        let op = instruction >> 12;
        match op {
            x if x == InstructionSet::ADD as u16 => {
                let dest_reg = (instruction >> 9) & 0x7; // destination register
                let operand_1_reg = (instruction >> 6) & 0x7;
                let immediate_mode = if (instruction >> 5) & 0x1 == 1 { true } else { false };
                if !immediate_mode {
                    let operand_2_reg = instruction & 0x7;
                    registers[dest_reg as usize] = registers[operand_1_reg as usize].wrapping_add(registers[operand_2_reg as usize]);
                } else {
                    let imm5 = instruction & 0x1F;
                    let imm5_sext = sign_extend(imm5, 5);
                    registers[dest_reg as usize] = registers[operand_1_reg as usize].wrapping_add(imm5_sext);
                }
                tracing.push(InstructionSet::ADD);
                update_flags(dest_reg, registers);
            }
            x if x == InstructionSet::ST as u16 => {
                let src_reg = (instruction >> 9) & 0x7;
                let pc_offset = instruction & 0x1FF;
                let pc_offset_sext = sign_extend(pc_offset, 9);
                let address = registers[REGISTER::PC as usize].wrapping_add(pc_offset_sext);
                let value = registers[src_reg as usize];
                write_to_memory(memory, address, value);
                tracing.push(InstructionSet::ST);
            }
            x if x == InstructionSet::JSR as u16 => {
                registers[REGISTER::R7 as usize] = registers[REGISTER::PC as usize];
                if ((instruction >> 11) & 0x1) == 0 {
                    let base_reg = (instruction >> 6) & 0x7;
                    registers[REGISTER::PC as usize] = registers[base_reg as usize]
                } else {
                    let pc_offset = instruction & 0x7FF;
                    let pc_offset_sext = sign_extend(pc_offset, 11);
                    registers[REGISTER::PC as usize] = registers[REGISTER::PC as usize].wrapping_add(pc_offset_sext);
                }
                tracing.push(InstructionSet::JSR);
            }
            x if x == InstructionSet::AND as u16 => {
                let dest_reg = (instruction >> 9) & 0x7;
                let operand_1_reg = (instruction >> 6) & 0x7;
                if ((instruction >> 5) & 0x1) == 0 {
                    let operand_2_reg = instruction & 0x7;
                    registers[dest_reg as usize] = registers[operand_1_reg as usize] & registers[operand_2_reg as usize];
                } else {
                    let imm5 = instruction & 0x1F;
                    let imm5_sext = sign_extend(imm5, 5);
                    registers[dest_reg as usize] = registers[operand_1_reg as usize] & (imm5_sext);
                }
                tracing.push(InstructionSet::AND);
                update_flags(dest_reg, registers);
            }
            x if x == InstructionSet::LDR as u16 => {
                let dest_reg = (instruction >> 9) & 0x7;
                let base_reg = (instruction >> 6) & 0x7;
                let offset_6 = instruction & 0x3F;
                let offset_6_sext = sign_extend(offset_6, 6);
                let address = registers[base_reg as usize].wrapping_add(offset_6_sext);
                registers[dest_reg as usize] = read_from_memory(memory, address);
                tracing.push(InstructionSet::LDR);
                update_flags(dest_reg, registers);
            }
            x if x == InstructionSet::LD as u16 => {
                let dest_reg = (instruction >> 9) & 0x7;
                let pc_offset = instruction & 0x1FF;
                let pc_offset_sext = sign_extend(pc_offset, 9);
                let address = registers[REGISTER::PC as usize].wrapping_add(pc_offset_sext);
                registers[dest_reg as usize] = read_from_memory(memory, address);
                tracing.push(InstructionSet::LD);
                update_flags(dest_reg, registers);
            }
            x if x == InstructionSet::LDI as u16 => {
                let dest_reg = (instruction >> 9) & 0x7;
                let pc_offset = instruction & 0x1FF;
                let pc_offset_sext = sign_extend(pc_offset, 9);
                let address_1 = registers[REGISTER::PC as usize].wrapping_add(pc_offset_sext);
                let address_2 = read_from_memory(memory, address_1);
                registers[dest_reg as usize] = read_from_memory(memory, address_2);
                tracing.push(InstructionSet::LDI);
                update_flags(dest_reg, registers);
            }
            x if x == InstructionSet::STR as u16 => {
                let src_reg = (instruction >> 9) & 0x7;
                let base_reg = (instruction >> 6) & 0x7;
                let offset_6 = instruction & 0x3F;
                let offset_6_sext = sign_extend(offset_6, 6);
                let address = registers[base_reg as usize].wrapping_add(offset_6_sext);
                let value = registers[src_reg as usize];
                write_to_memory(memory, address, value);
                tracing.push(InstructionSet::STR);
            }
            x if x == InstructionSet::NOT as u16 => {
                let dest_reg = (instruction >> 9) & 0x7;
                let operand_reg = (instruction >> 6) & 0x7;
                registers[dest_reg as usize] = !registers[operand_reg as usize];
                tracing.push(InstructionSet::NOT);
                update_flags(dest_reg, registers);
            }
            x if x == InstructionSet::STI as u16 => {
                let src_reg = (instruction >> 9) & 0x7;
                let pc_offset = instruction & 0x1FF;
                let pc_offset_sext = sign_extend(pc_offset, 9);
                let address_1 =  registers[REGISTER::PC as usize].wrapping_add(pc_offset_sext);
                let address_2 = read_from_memory(memory, address_1);
                let value = registers[src_reg as usize];
                write_to_memory(memory, address_2, value);
                tracing.push(InstructionSet::STI);
            }
            x if x == InstructionSet::JMP as u16 => {
                let base_reg = (instruction >> 6) & 0x7;
                registers[REGISTER::PC as usize] = registers[base_reg as usize];
                tracing.push(InstructionSet::JMP);
            }
            x if x == InstructionSet::LEA as u16 => {
                let dest_reg = (instruction >> 9) & 0x7;
                let pc_offset = instruction & 0x1FF;
                let pc_offset_sext = sign_extend(pc_offset, 9);
                registers[dest_reg as usize] = registers[REGISTER::PC as usize].wrapping_add(pc_offset_sext);
                tracing.push(InstructionSet::LEA);
                update_flags(dest_reg, registers);
            }
            x if x == InstructionSet::BR as u16 => {
                tracing.push(InstructionSet::BR);
                let cond_flag = (instruction >> 9) & 0x7;
                if (cond_flag & registers[REGISTER::COND as usize]) != 0 {
                    let pc_offset = instruction & 0x1FF;
                    let pc_offset_sext = sign_extend(pc_offset, 9);
                    registers[REGISTER::PC as usize] = registers[REGISTER::PC as usize].wrapping_add(pc_offset_sext);
                }
            }
            x if x == InstructionSet::TRAP as u16 => {
                registers[REGISTER::R7 as usize] = registers[REGISTER::PC as usize];
                let trap_code = instruction & 0xFF;
                tracing.push(InstructionSet::TRAP);
                match trap_code {
                    x if x == TrapCodes::GETC as u16 => {
                        while read_from_memory(memory, MemoryMappedRegisters::KBSR as u16) == 0 {}
                        let input_char = read_from_memory(memory, MemoryMappedRegisters::KBDR as u16);
                        registers[REGISTER::R0 as usize] = input_char;
                        update_flags(REGISTER::R0 as u16, registers);
                    }
                    x if x == TrapCodes::HALT as u16 => {
                        print!("HALT");
                        io::stdout().flush().unwrap();
                        running = false;
                    }
                    x if x == TrapCodes::IN as u16 => {
                        print!("Enter a character: ");
                        io::stdout().flush().unwrap();

                        while read_from_memory(memory, MemoryMappedRegisters::KBSR as u16) == 0 {}

                        let input_char = read_from_memory(memory, MemoryMappedRegisters::KBDR as u16);
                        registers[REGISTER::R0 as usize] = input_char;

                        println!("{}", input_char as u8 as char);
                        io::stdout().flush().unwrap();

                        update_flags(REGISTER::R0 as u16, registers);
                    }
                    x if x == TrapCodes::OUT as u16 => {
                        let character: u8 = (registers[REGISTER::R0 as usize] & 0xFF).try_into().unwrap();
                        print!("{}", character as char);
                        io::stdout().flush().unwrap();
                    }
                    x if x == TrapCodes::PUTS as u16 => {
                        let mut starting_addr = registers[REGISTER::R0 as usize];
                        let mut word: String = String::new();
                        while read_from_memory(memory, starting_addr) != 0 {
                            let character: u8 = (memory[starting_addr as usize] & 0xFF).try_into().unwrap();
                            word.push(character.try_into().unwrap());
                            starting_addr += 1;
                        }
                        print!("{}", word);
                        io::stdout().flush().unwrap();
                    }
                    x if x == TrapCodes::PUTSP as u16 => {
                        let mut starting_addr = registers[REGISTER::R0 as usize];
                        let mut word: String = String::new();
                        while read_from_memory(memory, starting_addr) != 0 {
                            let char_1: u8 = (memory[starting_addr as usize] & 0xFF).try_into().unwrap();
                            let char_2: u8 = (memory[starting_addr as usize] >> 8).try_into().unwrap();
                            word.push(char_1.try_into().unwrap());
                            if char_2 != 0 {
                                word.push(char_2.try_into().unwrap());
                            }
                            starting_addr += 1;
                        }
                        print!("{}", word);
                        io::stdout().flush().unwrap();
                    }
                    _ => {
                          
                    }
                }
            }
            x if (x == InstructionSet::RES as u16) | (x == InstructionSet::RTI as u16) => {
                panic!("Not implemented")
            }
            _ => {  }

        }
    }
}

fn main() {
    disable_input_buffering();

    // Get program from file in terminal
    let args: Vec<String> = env::args().collect();
    let file_path = &args[1];
    // Process file and get instruction
    let instructions = get_instructions(&file_path).unwrap();
    // Load to memory and initialize register
    let origin = instructions[0];
    let mut memory = load_memory(instructions);
    let mut registers = initialize_registers(origin);
    // Run program
    let mut tracing: Vec<InstructionSet> = Vec::new();
    run_program(&mut memory, &mut registers, &mut tracing);

    restore_input_buffering();
}
