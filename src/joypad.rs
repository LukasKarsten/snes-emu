use super::input::InputDevice;

#[derive(Default)]
pub struct JoypadIo {
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
            0x4016 => {
                let mut value = 0x00;
                if let Some(input) = &mut self.input1 {
                    value |= input.read_data1() as u8;
                    value |= (input.read_data2() as u8) << 1;
                }
                Some(value)
            }
            0x4017 => {
                let mut value = 0x1C;
                if let Some(input) = &mut self.input2 {
                    value |= input.read_data1() as u8;
                    value |= (input.read_data2() as u8) << 1;
                }
                Some(value)
            }
            _ => None,
        }
    }

    pub fn write(&mut self, addr: u32, value: u8) {
        if addr != 0x4016 {
            return;
        }
        if value & 1 != 0 {
            if let Some(input) = &mut self.input1 {
                input.strobe();
            }
            if let Some(input) = &mut self.input2 {
                input.strobe();
            }
        }
    }
}
