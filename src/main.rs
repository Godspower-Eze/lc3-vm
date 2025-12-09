use std::usize;

const MEMORY_SIZE: usize = 1 << 16;

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

enum ConditionFlags
{
    POS = 1 << 0, /* P */
    ZRO = 1 << 1, /* Z */
    NEG = 1 << 2, /* N */
}

enum TrapCodes {
    GETC = 0x20,  /* get character from keyboard, not echoed onto the terminal */
    OUT = 0x21,   /* output a character */
    PUTS = 0x22,  /* output a word string */
    IN = 0x23,    /* get character from keyboard, echoed onto the terminal */
    PUTSP = 0x24, /* output a byte string */
    HALT = 0x25   /* halt the program */
}

const PC_START: u16 = 0x3000; /* default starting position for the program counter */

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

fn main() {
    let mut memory: [u16; MEMORY_SIZE] = [0; MEMORY_SIZE];
    let mut registers: [u16; REGISTER::COUNT as usize] = [0; REGISTER::COUNT as usize];

    /* since exactly one condition flag should be set at any given time, set the Z flag */
    registers[REGISTER::COND as usize] = ConditionFlags::ZRO as u16;

    /* set the PC to starting position */
    registers[REGISTER::PC as usize] = PC_START;

    let mut running = true;

    while running {
        let pc = registers[REGISTER::PC as usize];
        let instruction = memory[pc as usize];
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
                update_flags(dest_reg, &mut registers);
            }
            x if x == InstructionSet::ST as u16 => {
                let src_reg = (instruction >> 9) & 0x7;
                let pc_offset = instruction & 0x1FF;
                let pc_offset_sext = sign_extend(pc_offset, 9);
                memory[((registers[REGISTER::PC as usize]).wrapping_add(pc_offset_sext)) as usize] = registers[src_reg as usize];
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
            }
            x if x == InstructionSet::AND as u16 => {
                let dest_reg = (instruction >> 9) & 0x7;
                let operand_1_reg = (instruction >> 6) & 0x7;
                if ((instruction >> 5) & 0x1) == 0 {
                    let operand_2_reg = instruction & 0x3;
                    registers[dest_reg as usize] = registers[operand_1_reg as usize].wrapping_add(registers[operand_2_reg as usize]);
                } else {
                    let imm5 = instruction & 0x1F;
                    let imm5_sext = sign_extend(imm5, 5);
                    registers[dest_reg as usize] = registers[operand_1_reg as usize] & (imm5_sext);
                }
                update_flags(dest_reg, &mut registers);
            }
            x if x == InstructionSet::LDR as u16 => {
                let dest_reg = (instruction >> 9) & 0x7;
                let base_reg = (instruction >> 6) & 0x7;
                let offset_6 = instruction & 0x3F;
                let offset_6_sext = sign_extend(offset_6, 6);
                registers[dest_reg as usize] = registers[(registers[base_reg as usize].wrapping_add(offset_6_sext)) as usize];
                update_flags(dest_reg, &mut registers);
            }
            x if x == InstructionSet::LD as u16 => {
                let dest_reg = (instruction >> 9) & 0x7;
                let pc_offset = instruction & 0x1FF;
                let pc_offset_sext = sign_extend(pc_offset, 9);
                registers[dest_reg as usize] = registers[((REGISTER::PC as u16).wrapping_add(pc_offset_sext)) as usize];
                update_flags(dest_reg, &mut registers);
            }
            x if x == InstructionSet::LDI as u16 => {
                let dest_reg = (instruction >> 9) & 0x7;
                let pc_offset = instruction & 0x1FF;
                let pc_offset_sext = sign_extend(pc_offset, 9);
                registers[dest_reg as usize] = registers[(registers[REGISTER::PC as usize].wrapping_add(pc_offset_sext)) as usize];
                update_flags(dest_reg, &mut registers);
            }
            x if x == InstructionSet::STR as u16 => {
                let src_reg = (instruction >> 9) & 0x7;
                let base_reg = (instruction >> 6) & 0x7;
                let offset_6 = instruction & 0x3F;
                let offset_6_sext = sign_extend(offset_6, 6);
                registers[(registers[base_reg as usize].wrapping_add(offset_6_sext)) as usize] = registers[src_reg as usize];
            }
            x if x == InstructionSet::NOT as u16 => {
                let dest_reg = (instruction >> 9) & 0x7;
                let operand_reg = (instruction >> 6) & 0x7;
                registers[dest_reg as usize] = !registers[operand_reg as usize];
            }
            x if x == InstructionSet::STI as u16 => {
                let src_reg = (instruction >> 9) & 0x7;
                let pc_offset = instruction & 0x1FF;
                let pc_offset_sext = sign_extend(pc_offset, 9);
                registers[registers[REGISTER::PC as usize].wrapping_add(pc_offset_sext) as usize] = registers[src_reg as usize];
            }
            x if x == InstructionSet::JMP as u16 => {
                let base_reg = (instruction >> 6) & 0x7;
                registers[REGISTER::PC as usize] = registers[base_reg as usize];
            }
            x if x == InstructionSet::LEA as u16 => {
                let dest_reg = (instruction >> 9) & 0x7;
                let pc_offset = instruction & 0x1FF;
                let pc_offset_sext = sign_extend(pc_offset, 9);
                registers[dest_reg as usize] = registers[registers[REGISTER::PC as usize].wrapping_add(pc_offset_sext) as usize];
                update_flags(dest_reg, &mut registers);
            }
            x if x == InstructionSet::BR as u16 => {
                let cond_flag = (instruction >> 9) & 0x7;
                if (cond_flag & registers[REGISTER::COND as usize]) != 0 {
                    let pc_offset = instruction & 0x1FF;
                    let pc_offset_sext = sign_extend(pc_offset, 9);
                    registers[REGISTER::PC as usize] = registers[registers[REGISTER::PC as usize].wrapping_add(pc_offset_sext) as usize];
                }
            }
            x if (x == InstructionSet::RES as u16) | (x == InstructionSet::RTI as u16) => {
                panic!("Not implemented")
            }
            _ => {  }

        }
    }
}
