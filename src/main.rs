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

const PC_START: u16 = 0x3000; /* default starting position for the program counter */

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
                let op_1 = (instruction >> 6) & 0x7;
                let immediate_mode = if (instruction >> 5) & 0x1 == 1 { true } else { false };
                if !immediate_mode {
                    let op_2 = instruction & 0x7;
                    registers[dest_reg as usize] = registers[op_1 as usize].wrapping_add(registers[op_2 as usize]);
                } else {
                    let imm5 = instruction & 0x1F;
                    let imm5_sext = if (imm5 >> 4) & 0x1 == 1 {
                        imm5 | 0xFFE0
                    } else {
                        imm5
                    };
                    registers[dest_reg as usize] = registers[op_1 as usize].wrapping_add(imm5_sext);
                }
            }
            _ => {  }

        }
    }
}
