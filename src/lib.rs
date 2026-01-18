use cpu::StepResult;
use input::InputDevice;

pub use apu::Apu;
pub use cpu::Cpu;
pub use dma::Dma;
pub use joypad::JoypadIo;
pub use ppu::{OutputImage, Ppu};
pub use wram::WRam;

pub mod apu;
pub mod cpu;
pub mod disasm;
pub mod dma;
pub mod input;
pub mod joypad;
pub mod ppu;
pub mod wram;

pub struct Snes {
    pub cpu: Cpu,
    pub ppu: Ppu,
    pub apu: Apu,
    wram: WRam,
    sram: Box<[u8; 0x080000]>,
    rom: Box<[u8]>,
    joypad: JoypadIo,
    pub dma: Dma,
    mdr: u8,
    frame_finished: bool,
}

impl Snes {
    pub fn new(rom: Box<[u8]>, mapping_mode: cpu::MappingMode) -> Self {
        let mut snes = Self {
            cpu: Cpu::new(mapping_mode),
            ppu: Ppu::default(),
            apu: Apu::default(),
            wram: WRam::default(),
            sram: vec![0; 0x080000].try_into().unwrap(),
            rom,
            joypad: JoypadIo::default(),
            dma: Dma::default(),
            mdr: 0,
            frame_finished: false,
        };
        snes.cpu.raise_interrupt(cpu::Interrupt::Reset);
        snes
    }

    pub fn set_input1(&mut self, input: Option<Box<dyn InputDevice>>) {
        self.joypad.input1 = input;
    }

    pub fn set_input2(&mut self, input: Option<Box<dyn InputDevice>>) {
        self.joypad.input2 = input;
    }

    pub fn output_image(&self) -> &OutputImage {
        self.ppu.output()
    }

    pub fn run(&mut self) -> bool {
        let mut ignore_breakpoints = true;

        if self.cpu.nmitimen_joypad_enable {
            fn read_input(
                input: &mut Option<Box<dyn InputDevice>>,
                joy1l: &mut u8,
                joy1h: &mut u8,
                joy2l: &mut u8,
                joy2h: &mut u8,
            ) {
                match input.as_deref_mut() {
                    Some(input) => {
                        input.strobe();
                        for _ in 0..8 {
                            *joy1h = (*joy1h << 1) | input.read_data1() as u8;
                            *joy2h = (*joy2h << 1) | input.read_data2() as u8;
                        }
                        for _ in 0..8 {
                            *joy1l = (*joy1l << 1) | input.read_data1() as u8;
                            *joy2l = (*joy2l << 1) | input.read_data2() as u8;
                        }
                    }
                    None => {
                        *joy1h = 0;
                        *joy2h = 0;
                    }
                }
            }

            read_input(
                &mut self.joypad.input1,
                &mut self.cpu.joy1l,
                &mut self.cpu.joy1h,
                &mut self.cpu.joy2l,
                &mut self.cpu.joy2h,
            );
            read_input(
                &mut self.joypad.input2,
                &mut self.cpu.joy3l,
                &mut self.cpu.joy3h,
                &mut self.cpu.joy4l,
                &mut self.cpu.joy4h,
            );
            self.cpu.hvbjoy_auto_joypad_read_busy_flag = false;
        }

        while !self.frame_finished {
            let result = cpu::step(self, ignore_breakpoints);
            ignore_breakpoints = false;

            if result == StepResult::BreakpointHit {
                return true;
            }
        }

        self.frame_finished = false;
        false
    }

    pub fn step(&mut self) -> StepResult {
        let result = cpu::step(self, true);
        ppu::catch_up(self);
        apu::catch_up(self);
        result
    }
}
