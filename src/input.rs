pub trait InputDevice {
    fn strobe(&mut self);
    fn read_data1(&mut self) -> bool;
    fn read_data2(&mut self) -> bool {
        false
    }
}

#[derive(Default, Clone, Copy)]
pub struct JoypadState {
    pub button_b: bool,
    pub button_y: bool,
    pub button_select: bool,
    pub button_start: bool,
    pub dpad_up: bool,
    pub dpad_down: bool,
    pub dpad_left: bool,
    pub dpad_right: bool,
    pub button_a: bool,
    pub button_x: bool,
    pub button_l: bool,
    pub button_r: bool,
}

pub struct Joypad<F> {
    updater: F,
    buffer: u16,
}

impl<F> Joypad<F> {
    pub fn new(updater: F) -> Self {
        Self { updater, buffer: 0 }
    }
}

impl<F: FnMut() -> JoypadState> InputDevice for Joypad<F> {
    #[allow(clippy::identity_op)]
    fn strobe(&mut self) {
        let state = (self.updater)();
        self.buffer = 0;
        self.buffer |= (state.button_b as u16) << 0;
        self.buffer |= (state.button_y as u16) << 1;
        self.buffer |= (state.button_select as u16) << 2;
        self.buffer |= (state.button_start as u16) << 3;
        self.buffer |= (state.dpad_up as u16) << 4;
        self.buffer |= (state.dpad_down as u16) << 5;
        self.buffer |= (state.dpad_left as u16) << 6;
        self.buffer |= (state.dpad_right as u16) << 7;
        self.buffer |= (state.button_a as u16) << 8;
        self.buffer |= (state.button_x as u16) << 9;
        self.buffer |= (state.button_l as u16) << 10;
        self.buffer |= (state.button_r as u16) << 11;
    }

    fn read_data1(&mut self) -> bool {
        let value = (self.buffer & 1) != 0;
        self.buffer = (self.buffer >> 1) | 0x8000;
        value
    }
}
