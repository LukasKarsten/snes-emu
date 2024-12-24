pub struct WRam {
    wmadd: u32,
    pub data: Box<[u8; 0x020000]>,
}

impl Default for WRam {
    fn default() -> Self {
        Self {
            data: vec![0; 0x020000].try_into().unwrap(),
            wmadd: 0,
        }
    }
}

impl WRam {
    pub fn read_pure(&self, addr: u32) -> Option<u8> {
        match addr {
            0x2180 => Some(self.data[self.wmadd as usize]),
            _ => None,
        }
    }

    pub fn read(&mut self, addr: u32) -> Option<u8> {
        match addr {
            0x2180 => {
                let addr = self.wmadd;
                self.wmadd = (self.wmadd + 1) & 0x01FFFF;
                Some(self.data[addr as usize])
            }
            _ => None,
        }
    }

    pub fn write(&mut self, addr: u32, value: u8) {
        match addr {
            0x2180 => {
                self.data[self.wmadd as usize] = value;
                self.wmadd = (self.wmadd + 1) & 0x01FFFF;
            }
            0x2181 => self.wmadd = self.wmadd & 0x01FF00 | (value as u32),
            0x2182 => self.wmadd = self.wmadd & 0x0100FF | (value as u32) << 8,
            0x2183 => self.wmadd = self.wmadd & 0x00FFFF | ((value as u32) & 1) << 16,
            _ => (),
        }
    }
}
