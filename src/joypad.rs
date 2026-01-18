use super::input::InputDevice;

#[derive(Default)]
pub struct JoypadIo {
    joywr: u8,
    pub input1: Option<Box<dyn InputDevice>>,
    pub input2: Option<Box<dyn InputDevice>>,
}

impl JoypadIo {
    pub fn read_pure(&self, addr: u32) -> Option<u8> {
        match addr {
            0x4016 => Some(0x00),
            0x4017 => Some(0x1F),
            _ => None,
        }
    }

    pub fn read(&mut self, addr: u32) -> Option<u8> {
        match addr {
            // TODO: read input
            0x4016 => Some(0x03),
            0x4017 => Some(0x1F),
            _ => None,
        }
    }

    pub fn write(&mut self, addr: u32, value: u8) {
        if addr == 0x4016 {
            self.joywr = value
        }
    }
}
