use arbitrary_int::*;

use super::Bus;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum OBSELSizeSelection {
    Small8x8Large16x16,
    Small8x8Large32x32,
    Small8x8Large64x64,
    Small16x16Large32x32,
    Small16x16Large64x64,
    Small32x32Large64x64,
    Small16x32Large32x64,
    Small16x32Large32x32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum VMAINIncrementMode {
    Low,
    High,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum VMAINAddressTranslation {
    None,
    Bit8,
    Bit9,
    Bit10,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum VMAINAddressIncrementStep {
    Step1,
    Step32,
    Step128,
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum BackgroundSize {
    #[default]
    OneScreen,
    VMirror,
    HMirror,
    FourScreen,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum M7SELScreenOver {
    Wrap,
    Transparent,
    Tile0,
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MathEnable {
    Always,
    InsideWindow,
    OutsideWindow,
    Never,
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum MathOperation {
    #[default]
    Add,
    Sub,
}

#[derive(Default, Clone, Copy)]
pub struct Background {
    pub size: BackgroundSize,
    /// 1k word-steps
    pub base_address: u6,
    pub large_tiles: bool,
    /// 4k word-steps
    pub tile_base_address: u4,
    /// only lower 10 bits used
    pub h_offset: u16,
    /// only lower 10 bits used
    pub v_offset: u16,
    pub mosaic: bool,
}

#[derive(Clone, Copy)]
pub struct Backgrounds {
    pub mode: u3,
    // TODO: consider moving this into `Screens`
    pub bg3_high_priority: bool,
    pub mosaic_size: u4,
    pub direct_color: bool,
    pub backgrounds: [Background; 4],
}

impl Default for Backgrounds {
    fn default() -> Self {
        Self {
            mode: u3::new(7),
            bg3_high_priority: true,
            mosaic_size: u4::new(0),
            direct_color: false,
            backgrounds: [Background::default(); 4],
        }
    }
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum WindowMaskLogic {
    #[default]
    Or,
    And,
    Xor,
    Xnor,
}

impl WindowMaskLogic {
    fn from_bits(bits: u2) -> Self {
        match bits.value() {
            0 => Self::Or,
            1 => Self::And,
            2 => Self::Xor,
            3 => Self::Xnor,
            _ => panic!(),
        }
    }
}

const WINDOW_OBJ: u8 = 0x01;
const WINDOW_BG1: u8 = 0x02;
const WINDOW_BG2: u8 = 0x04;
const WINDOW_BG3: u8 = 0x08;
const WINDOW_BG4: u8 = 0x10;
const WINDOW_MATH: u8 = 0x20;

#[derive(Clone, Copy)]
pub struct Windows {
    pub window1_left: u8,
    pub window1_right: u8,
    pub window2_left: u8,
    pub window2_right: u8,
    pub tmw: u8,
    pub tsw: u8,
    pub w1en: u8,
    pub w2en: u8,
    pub w1inv: u8,
    pub w2inv: u8,
    pub main_screen_black: MathEnable,
    pub sub_screen_black: MathEnable,
    pub backgrounds: [WindowMaskLogic; 4],
    pub objects: WindowMaskLogic,
    pub math: WindowMaskLogic,
}

impl Default for Windows {
    fn default() -> Self {
        Self {
            window1_left: 0,
            window1_right: 0,
            window2_left: 0,
            window2_right: 0,
            tmw: 0,
            tsw: 0,
            w1en: 0,
            w1inv: 0,
            w2en: 0,
            w2inv: 0,
            main_screen_black: MathEnable::Never,
            sub_screen_black: MathEnable::Always,
            backgrounds: [WindowMaskLogic::default(); 4],
            objects: WindowMaskLogic::default(),
            math: WindowMaskLogic::default(),
        }
    }
}

#[derive(Default, Clone, Copy)]
pub struct Screens {
    pub tm: u8,
    pub ts: u8,
    pub sub_screen_bg_obj_enable: bool,
    pub math_operation: MathOperation,
    pub half: bool,
    pub math_on_backgrounds: [bool; 4],
    pub math_on_backdrop: bool,
    pub math_on_objects: bool,
    pub backdrop_red: u5,
    pub backdrop_green: u5,
    pub backdrop_blue: u5,
}

const LAYER_BG1: u8 = 0;
const LAYER_BG2: u8 = 1;
const LAYER_BG3: u8 = 2;
const LAYER_BG4: u8 = 3;
const LAYER_OBJ: u8 = 4;
const LAYER_BACKDROP: u8 = 5;
const NUM_LAYERS: usize = 6;

#[derive(Debug, Clone, Copy, bytemuck::NoUninit)]
#[repr(C)]
struct OutputColor {
    red: u8,
    green: u8,
    blue: u8,
    // NOTE: this is always `255`. This field just exists so we can send this struct directly to
    // the GPU as RGBA.
    alpha: u8,
}

impl OutputColor {
    const BLACK: Self = Self::from_rgb(u5::new(0), u5::new(0), u5::new(0));

    const fn from_rgb(red: u5, green: u5, blue: u5) -> Self {
        Self {
            red: red.value(),
            green: green.value(),
            blue: blue.value(),
            alpha: 255,
        }
    }
}

#[derive(Debug, Clone)]
pub struct OutputImage(Box<[OutputColor; Self::MAX_PIXELS]>);

impl Default for OutputImage {
    fn default() -> Self {
        Self(
            vec![OutputColor::BLACK; Self::MAX_PIXELS]
                .try_into()
                .unwrap(),
        )
    }
}

impl OutputImage {
    pub const WIDTH: u16 = 512;
    pub const MAX_HEIGHT: u16 = 478;
    pub const MIN_HEIGHT: u16 = 224;
    pub const MAX_PIXELS: usize = Self::WIDTH as usize * Self::MAX_HEIGHT as usize;

    fn set(&mut self, x: u16, y: u16, color: OutputColor) {
        assert!(x < 256 * 2);
        assert!(y < 239 * 2);
        let idx = usize::from(x) | (usize::from(y) * 512);
        self.0[idx] = color;
    }

    pub fn pixels_rgba(&self) -> &[u8] {
        bytemuck::cast_slice(&self.0[..])
    }
}

pub struct PpuIo {
    ////////////////////////////////////////////////////////////////////////////
    // write-only
    pub backgrounds: Backgrounds,
    pub windows: Windows,
    pub screens: Screens,

    pub inidisp_forced_blanking: bool,
    pub inidisp_master_brightness: u4,
    /// Size of large and small sprite tiles
    pub obsel_size_selection: OBSELSizeSelection,
    pub obsel_gap: u2,
    pub obsel_base_address: u3,
    // OAMADD
    pub oamaddl: u8,
    pub oamaddh: u8,
    // BGnHOFS / BGnVOFS
    pub m7hofs: u16,
    pub m7vofs: u16,
    // VMAIN
    pub vmain_increment_mode: VMAINIncrementMode,
    pub vmain_address_translation: VMAINAddressTranslation,
    pub vmain_address_increment_step: VMAINAddressIncrementStep,
    // VMADD
    pub vmadd: u16,
    // VMDATAL / VMDATAH
    pub vmdatal: u8,
    pub vmdatah: u8,
    // M7SEL
    pub m7sel_screen_over: M7SELScreenOver,
    pub m7sel_screen_vflip: bool,
    pub m7sel_screen_hflip: bool,
    // M7x
    pub m7a: u16,
    pub m7b: u16,
    pub m7c: u16,
    pub m7d: u16,
    pub m7x: u16,
    pub m7y: u16,
    // CGADD
    pub cgadd: u8,
    // SETINI
    pub setini_interlace: bool,
    pub setini_interlace_obj_highvres: bool,
    pub setini_overscan: bool,
    pub setini_hpseudo512: bool,
    pub setini_extbg: bool,
    pub setini_external_sync: bool,

    ////////////////////////////////////////////////////////////////////////////
    // read-only
    pub mpyl: u8,
    pub mpym: u8,
    pub mpyh: u8,
    pub ophct: u16,
    pub opvct: u16,
    pub stat77: u8,
    pub stat78: u8,
    ////////////////////////////////////////////////////////////////////////////
    // internal
    pub oam: Box<[u8; 0x220]>,
    oam_addr: u16,
    pub vram: Box<[u8; 0x10000]>,
    pub cgram: Box<[u8; 0x200]>,
    cgram_selector: u8,
    bg_old: u8,
    m7_old: u8,
    ophct_selector: u8,
    opvct_selector: u8,
}

impl Default for PpuIo {
    fn default() -> Self {
        Self {
            backgrounds: Backgrounds::default(),
            windows: Windows::default(),
            screens: Screens::default(),

            inidisp_forced_blanking: true,
            inidisp_master_brightness: u4::MAX,
            obsel_size_selection: OBSELSizeSelection::Small8x8Large16x16,
            obsel_gap: u2::new(0),
            obsel_base_address: u3::new(0),
            oamaddl: 0,
            oamaddh: 0,
            m7hofs: 0,
            m7vofs: 0,
            vmain_increment_mode: VMAINIncrementMode::Low,
            vmain_address_translation: VMAINAddressTranslation::Bit10,
            vmain_address_increment_step: VMAINAddressIncrementStep::Step128,
            vmadd: 0,
            vmdatal: 0,
            vmdatah: 0,
            m7sel_screen_over: M7SELScreenOver::Wrap,
            m7sel_screen_vflip: false,
            m7sel_screen_hflip: false,
            m7a: 0xFF,
            m7b: 0xFF,
            m7c: 0,
            m7d: 0,
            m7x: 0,
            m7y: 0,
            cgadd: 0,
            setini_interlace: false,
            setini_interlace_obj_highvres: false,
            setini_overscan: false,
            setini_hpseudo512: false,
            setini_extbg: false,
            setini_external_sync: false,

            mpyl: 0x01,
            mpym: 0x00,
            mpyh: 0x00,
            ophct: 0x01FF,
            opvct: 0x01FF,
            stat77: 0x00,
            stat78: 0x00,

            oam: vec![0; 0x220].try_into().unwrap(),
            oam_addr: 0,
            vram: vec![0; 0x10000].try_into().unwrap(),
            cgram: vec![0; 0x200].try_into().unwrap(),
            cgram_selector: 0,
            bg_old: 0,
            m7_old: 0,
            ophct_selector: 0,
            opvct_selector: 0,
        }
    }
}

impl PpuIo {
    pub fn read_pure(&self, addr: u32) -> Option<u8> {
        let value = match addr {
            0x2134 => self.mpyl,
            0x2135 => self.mpym,
            0x2136 => self.mpyh,
            0x2104 => {
                let addr = usize::from(self.oam_addr);
                if addr >= self.oam.len() {
                    return None;
                }
                self.oam[addr]
            }
            0x2139 => self.vmdatal,
            0x213A => self.vmdatah,
            0x213B => {
                let addr = usize::from(self.cgadd) * 2 + usize::from(self.cgram_selector);
                self.cgram[addr]
            }
            0x213C => (self.ophct >> self.ophct_selector) as u8,
            0x213D => (self.opvct >> self.opvct_selector) as u8,
            0x213E => self.stat77,
            0x213F => self.stat78,
            _ => return None,
        };

        Some(value)
    }

    pub fn read(&mut self, addr: u32) -> Option<u8> {
        let value = match addr {
            0x2134 => self.mpyl,
            0x2135 => self.mpym,
            0x2136 => self.mpyh,
            0x2137 => {
                // TODO: Latch H/V Counter
                return None;
            }
            0x2138 => {
                let addr = usize::from(self.oam_addr);
                self.oam_addr = self.oam_addr.wrapping_add(1);
                if addr >= self.oam.len() {
                    return None;
                }
                self.oam[addr]
            }
            0x2139 => {
                let value = self.vmdatal;
                if self.vmain_increment_mode == VMAINIncrementMode::Low {
                    self.prefetch_vmadd();
                    self.increment_vmadd();
                }
                value
            }
            0x213A => {
                let value = self.vmdatah;
                if self.vmain_increment_mode == VMAINIncrementMode::High {
                    self.prefetch_vmadd();
                    self.increment_vmadd();
                }
                value
            }
            0x213B => {
                let addr = usize::from(self.cgadd) * 2 + usize::from(self.cgram_selector);
                self.cgadd = self.cgadd.wrapping_add(self.cgram_selector);
                self.cgram_selector ^= 1;
                self.cgram[addr]
            }
            0x213C => {
                let value = (self.ophct >> self.ophct_selector) as u8;
                self.ophct_selector ^= 8;
                value
            }
            0x213D => {
                let value = (self.opvct >> self.opvct_selector) as u8;
                self.opvct_selector ^= 8;
                value
            }
            0x213E => self.stat77,
            0x213F => self.stat78,
            _ => return None,
        };

        Some(value)
    }

    pub fn write(&mut self, addr: u32, value: u8) {
        match addr {
            0x2100 => {
                self.inidisp_forced_blanking = value & 0x80 != 0;
                self.inidisp_master_brightness = u4::extract_u8(value, 0);
            }
            0x2101 => {
                self.obsel_size_selection = match value >> 5 {
                    0 => OBSELSizeSelection::Small8x8Large16x16,
                    1 => OBSELSizeSelection::Small8x8Large32x32,
                    2 => OBSELSizeSelection::Small8x8Large64x64,
                    3 => OBSELSizeSelection::Small16x16Large32x32,
                    4 => OBSELSizeSelection::Small16x16Large64x64,
                    5 => OBSELSizeSelection::Small32x32Large64x64,
                    6 => OBSELSizeSelection::Small16x32Large32x64,
                    7 => OBSELSizeSelection::Small16x32Large32x32,
                    _ => unreachable!(),
                };
                self.obsel_gap = u2::extract_u8(value, 3);
                self.obsel_base_address = u3::extract_u8(value, 0);
            }
            0x2102 => {
                self.oamaddl = value;
                self.oam_addr = (self.oamaddh as u16) << 9 | (self.oamaddl as u16) << 1;
            }
            0x2103 => {
                self.oamaddh = value & 1;
                self.oam_addr = (self.oamaddh as u16) << 9 | (self.oamaddl as u16) << 1;
            }
            0x2104 => {
                let addr = usize::from(self.oam_addr);
                if addr < self.oam.len() {
                    self.oam[addr] = value;
                }
                self.oam_addr = self.oam_addr.wrapping_add(1);
            }
            0x2105 => {
                self.backgrounds.backgrounds[3].large_tiles = value & 0x80 != 0;
                self.backgrounds.backgrounds[2].large_tiles = value & 0x40 != 0;
                self.backgrounds.backgrounds[1].large_tiles = value & 0x20 != 0;
                self.backgrounds.backgrounds[0].large_tiles = value & 0x10 != 0;
                self.backgrounds.bg3_high_priority = value & 0x08 != 0;
                self.backgrounds.mode = u3::extract_u8(value, 0);
            }
            0x2106 => {
                self.backgrounds.mosaic_size = u4::extract_u8(value, 4);
                self.backgrounds.backgrounds[3].mosaic = value & 0x08 != 0;
                self.backgrounds.backgrounds[2].mosaic = value & 0x04 != 0;
                self.backgrounds.backgrounds[1].mosaic = value & 0x02 != 0;
                self.backgrounds.backgrounds[0].mosaic = value & 0x01 != 0;
            }
            0x2107..=0x210A => {
                let bg = &mut self.backgrounds.backgrounds[(addr - 0x2107) as usize];
                bg.base_address = u6::extract_u8(value, 2);
                bg.size = match value & 0x03 {
                    0 => BackgroundSize::OneScreen,
                    1 => BackgroundSize::VMirror,
                    2 => BackgroundSize::HMirror,
                    3 => BackgroundSize::FourScreen,
                    _ => unreachable!(),
                }
            }
            0x210B => {
                self.backgrounds.backgrounds[1].tile_base_address = u4::extract_u8(value, 4);
                self.backgrounds.backgrounds[0].tile_base_address = u4::extract_u8(value, 0);
            }
            0x210C => {
                self.backgrounds.backgrounds[3].tile_base_address = u4::extract_u8(value, 4);
                self.backgrounds.backgrounds[2].tile_base_address = u4::extract_u8(value, 0);
            }
            0x210D..=0x2114 => {
                let background = &mut self.backgrounds.backgrounds[((addr - 0x210D) / 2) as usize];
                if addr % 2 == 1 {
                    // FIXME: Not sure if this is correct.
                    background.h_offset = (value as u16) << 8
                        | ((self.bg_old & !7) as u16)
                        | (background.h_offset >> 8 & 7);
                    self.bg_old = value;

                    if addr == 0x210D {
                        self.m7hofs = (value as u16) << 8 | self.m7_old as u16;
                        self.m7_old = value;
                    }
                } else {
                    background.v_offset = ((value as u16) << 8) | self.bg_old as u16;
                    self.bg_old = value;

                    if addr == 0x210E {
                        self.m7vofs = (value as u16) << 8 | self.m7_old as u16;
                        self.m7_old = value;
                    }
                }
            }
            0x2115 => {
                self.vmain_increment_mode = match value >> 7 {
                    0 => VMAINIncrementMode::Low,
                    1 => VMAINIncrementMode::High,
                    _ => unreachable!(),
                };
                self.vmain_address_translation = match value >> 2 & 0x03 {
                    0 => VMAINAddressTranslation::None,
                    1 => VMAINAddressTranslation::Bit8,
                    2 => VMAINAddressTranslation::Bit9,
                    3 => VMAINAddressTranslation::Bit10,
                    _ => unreachable!(),
                };
                self.vmain_address_increment_step = match value & 0x03 {
                    0 => VMAINAddressIncrementStep::Step1,
                    1 => VMAINAddressIncrementStep::Step32,
                    2 | 3 => VMAINAddressIncrementStep::Step128,
                    _ => unreachable!(),
                };
            }
            0x2116 => {
                self.vmadd = (self.vmadd & 0xFF00) | (value as u16);
                self.prefetch_vmadd();
            }
            0x2117 => {
                self.vmadd = (self.vmadd & 0x00FF) | (value as u16) << 8;
                self.prefetch_vmadd();
            }
            0x2118 => {
                self.vram[usize::from(self.translated_vram_word_address() << 1)] = value;
                if self.vmain_increment_mode == VMAINIncrementMode::Low {
                    self.increment_vmadd();
                }
            }
            0x2119 => {
                self.vram[usize::from(self.translated_vram_word_address() << 1) | 1] = value;
                if self.vmain_increment_mode == VMAINIncrementMode::High {
                    self.increment_vmadd();
                }
            }
            0x211A => {
                self.m7sel_screen_over = match value >> 6 {
                    0 | 1 => M7SELScreenOver::Wrap,
                    2 => M7SELScreenOver::Transparent,
                    3 => M7SELScreenOver::Tile0,
                    _ => unreachable!(),
                };
                self.m7sel_screen_vflip = value & 0x2 != 0;
                self.m7sel_screen_hflip = value & 0x1 != 0;
            }
            0x211B => {
                // TODO: This port can also be used for general purpose math multiply.
                // FIXME: I have no idea if this is the correct way.
                self.m7a = ((value as u16) << 8) | self.bg_old as u16;
                self.bg_old = value;
            }
            0x211C => {
                self.m7b = ((value as u16) << 8) | self.bg_old as u16;
                self.bg_old = value;
            }
            0x211D => {
                self.m7c = ((value as u16) << 8) | self.bg_old as u16;
                self.bg_old = value;
            }
            0x211E => {
                self.m7d = ((value as u16) << 8) | self.bg_old as u16;
                self.bg_old = value;
            }
            0x211F => {
                self.m7x = ((value as u16) << 8) | self.bg_old as u16;
                self.bg_old = value;
            }
            0x2120 => {
                self.m7y = ((value as u16) << 8) | self.bg_old as u16;
                self.bg_old = value;
            }
            0x2121 => {
                self.cgadd = value;
                self.cgram_selector = 0;
            }
            0x2122 => {
                self.cgram[usize::from(self.cgadd) * 2 + usize::from(self.cgram_selector)] = value;
                self.cgadd = self.cgadd.wrapping_add(self.cgram_selector);
                self.cgram_selector ^= 1;
            }
            0x2123 => {
                self.windows.w1en &= !0x06;
                self.windows.w2en &= !0x06;
                self.windows.w1inv &= !0x06;
                self.windows.w2inv &= !0x06;

                // bit 2
                self.windows.w1inv |= (value & 0x01) << 1;
                self.windows.w1en |= (value & 0x02) >> 0;
                self.windows.w2inv |= (value & 0x04) >> 1;
                self.windows.w2en |= (value & 0x08) >> 2;

                // bit 3
                self.windows.w1inv |= (value & 0x10) >> 2;
                self.windows.w1en |= (value & 0x20) >> 3;
                self.windows.w2inv |= (value & 0x40) >> 4;
                self.windows.w2en |= (value & 0x80) >> 5;
            }
            0x2124 => {
                self.windows.w1en &= !0x18;
                self.windows.w2en &= !0x18;
                self.windows.w1inv &= !0x18;
                self.windows.w2inv &= !0x18;

                // bit 4
                self.windows.w1inv |= (value & 0x01) << 3;
                self.windows.w1en |= (value & 0x02) << 2;
                self.windows.w2inv |= (value & 0x04) << 1;
                self.windows.w2en |= (value & 0x08) << 0;

                // bit 5
                self.windows.w1inv |= (value & 0x10) >> 0;
                self.windows.w1en |= (value & 0x20) >> 1;
                self.windows.w2inv |= (value & 0x40) >> 2;
                self.windows.w2en |= (value & 0x80) >> 3;
            }
            0x2125 => {
                self.windows.w1en &= !0x21;
                self.windows.w2en &= !0x21;
                self.windows.w1inv &= !0x21;
                self.windows.w2inv &= !0x21;

                // bit 1
                self.windows.w1inv |= (value & 0x01) >> 0;
                self.windows.w1en |= (value & 0x02) >> 1;
                self.windows.w2inv |= (value & 0x04) >> 2;
                self.windows.w2en |= (value & 0x08) >> 3;

                // bit 6
                self.windows.w1inv |= (value & 0x10) << 1;
                self.windows.w1en |= (value & 0x20) << 0;
                self.windows.w2inv |= (value & 0x40) >> 1;
                self.windows.w2en |= (value & 0x80) >> 2;
            }
            0x2126 => self.windows.window1_left = value,
            0x2127 => self.windows.window1_right = value,
            0x2128 => self.windows.window2_left = value,
            0x2129 => self.windows.window2_right = value,
            0x212A => {
                for (i, logic) in self.windows.backgrounds.iter_mut().enumerate() {
                    *logic = WindowMaskLogic::from_bits(u2::extract_u8(value, i * 2));
                }
            }
            0x212B => {
                self.windows.objects = WindowMaskLogic::from_bits(u2::extract_u8(value, 0));
                self.windows.math = WindowMaskLogic::from_bits(u2::extract_u8(value, 2));
            }
            0x212C => self.screens.tm = value,
            0x212D => self.screens.ts = value,
            0x212E => self.windows.tmw = value,
            0x212F => self.windows.tsw = value,
            0x2130 => {
                self.backgrounds.direct_color = value & 0x01 != 0;
                self.screens.sub_screen_bg_obj_enable = value & 0x02 != 0;
                self.windows.sub_screen_black = match value >> 4 & 0x03 {
                    0 => MathEnable::Always,
                    1 => MathEnable::InsideWindow,
                    2 => MathEnable::OutsideWindow,
                    3 => MathEnable::Never,
                    _ => unreachable!(),
                };
                self.windows.main_screen_black = match value >> 6 & 0x03 {
                    0 => MathEnable::Never,
                    1 => MathEnable::InsideWindow,
                    2 => MathEnable::OutsideWindow,
                    3 => MathEnable::Always,
                    _ => unreachable!(),
                };
            }
            0x2131 => {
                self.screens.math_operation = match value >> 7 {
                    0 => MathOperation::Add,
                    1 => MathOperation::Sub,
                    _ => unreachable!(),
                };
                self.screens.half = value & 0x40 != 0;
                self.screens.math_on_backdrop = value & 0x20 != 0;
                self.screens.math_on_objects = value & 0x10 != 0;
                self.screens.math_on_backgrounds[3] = value & 0x08 != 0;
                self.screens.math_on_backgrounds[2] = value & 0x04 != 0;
                self.screens.math_on_backgrounds[1] = value & 0x02 != 0;
                self.screens.math_on_backgrounds[0] = value & 0x01 != 0;
            }
            0x2132 => {
                let intensity = u5::extract_u8(value, 0);
                if value & 0x20 != 0 {
                    self.screens.backdrop_red = intensity;
                }
                if value & 0x40 != 0 {
                    self.screens.backdrop_green = intensity;
                }
                if value & 0x80 != 0 {
                    self.screens.backdrop_blue = intensity;
                }
            }
            0x2133 => {
                self.setini_interlace = value & 0x01 != 0;
                self.setini_interlace_obj_highvres = value & 0x02 != 0;
                self.setini_overscan = value & 0x04 != 0;
                self.setini_hpseudo512 = value & 0x08 != 0;
                self.setini_extbg = value & 0x40 != 0;
                self.setini_external_sync = value & 0x80 != 0;
            }
            _ => (),
        }
    }

    fn translated_vram_word_address(&self) -> u16 {
        let n = match self.vmain_address_translation {
            VMAINAddressTranslation::None => return self.vmadd,
            VMAINAddressTranslation::Bit8 => 8,
            VMAINAddressTranslation::Bit9 => 9,
            VMAINAddressTranslation::Bit10 => 10,
        };

        let temp = self.vmadd.rotate_right(n - 3);
        let rotated = (temp & 0x7) | temp >> (16 - n);

        (self.vmadd & (!0 << n)) | rotated
    }

    fn increment_vmadd(&mut self) {
        let step = match self.vmain_address_increment_step {
            VMAINAddressIncrementStep::Step1 => 1,
            VMAINAddressIncrementStep::Step32 => 32,
            VMAINAddressIncrementStep::Step128 => 128,
        };
        self.vmadd = self.vmadd.wrapping_add(step);
    }

    fn prefetch_vmadd(&mut self) {
        let word_addr = self.translated_vram_word_address();
        self.vmdatal = self.vram[usize::from(word_addr << 1)];
        self.vmdatah = self.vram[usize::from(word_addr << 1) + 1];
    }

    pub fn reset(&mut self) {
        self.inidisp_forced_blanking = true;
        self.setini_interlace = false;
        self.setini_overscan = false;
        self.setini_interlace_obj_highvres = false;
        self.setini_hpseudo512 = false;
        self.setini_extbg = false;
        self.setini_external_sync = false;
    }

    pub fn output_height(&self) -> u16 {
        match self.setini_overscan {
            false => 224,
            true => 239,
        }
    }
}

pub struct Ppu {
    pending_cycles: u64,
    pub hpos: u16,
    pub vpos: u16,
    output: OutputImage,
}

impl Default for Ppu {
    fn default() -> Self {
        Self {
            pending_cycles: 0,
            hpos: 0,
            vpos: 0,
            output: OutputImage::default(),
        }
    }
}

impl Ppu {
    pub fn output(&self) -> &OutputImage {
        &self.output
    }

    pub fn step(&mut self, bus: &mut Bus, cycles: u64) -> bool {
        self.pending_cycles += cycles;

        /*
        let width = match ppu.setini_hpseudo512 {
            false => 256,
            true => 512,
        };

        let height = match (ppu.setini_overscan, ppu.setini_interlace) {
            (false, false) => 224,
            (false, true) => 448,
            (true, false) => 239,
            (true, true) => 478,
        };
        */

        let height = bus.ppu.output_height();

        // TODO: This is not acutally dependent on the height but rather whether the console is a NTSC
        // or PAL console. (at least I think so ..)
        let screen_height = height;

        if bus.ppu.setini_interlace {
            todo!()
        }
        if bus.ppu.setini_hpseudo512 {
            todo!()
        }

        while self.pending_cycles >= 4 {
            self.pending_cycles -= 4;

            self.hpos += 1;
            if self.hpos > 339 {
                self.hpos = 0;
                self.vpos += 1;

                if self.vpos == 2 {
                    bus.cpu.set_vblank_nmi_flag(false);
                } else if self.vpos == height + 1 {
                    bus.cpu.set_vblank_nmi_flag(true);
                }

                if self.vpos > screen_height + 37 {
                    self.vpos = 0;
                }
            }

            let hblank = self.hpos < 22 || self.hpos > 277;
            let vblank = self.vpos < 1 || self.vpos > height;

            bus.cpu.hvbjoy_hblank_period_flag = hblank;
            bus.cpu.hvbjoy_vblank_period_flag = vblank;

            let h_irq = self.hpos == bus.cpu.htime.value();
            let v_irq = self.vpos == bus.cpu.vtime.value();
            // PERF: We could eliminate this match with some bit fiddling
            match bus.cpu.nmitimen_hv_irq {
                crate::cpu::HvIrq::Disable => (),
                crate::cpu::HvIrq::Horizontal => bus.cpu.timeup_hv_count_timer_irq_flag = h_irq,
                crate::cpu::HvIrq::Vertical => bus.cpu.timeup_hv_count_timer_irq_flag = v_irq,
                crate::cpu::HvIrq::End => bus.cpu.timeup_hv_count_timer_irq_flag = h_irq & v_irq,
            }

            if !hblank && !vblank {
                let x = self.hpos - 22;
                let y = self.vpos - 1;

                let color = match bus.ppu.inidisp_forced_blanking {
                    false => self.render_pixel(&bus.ppu, x, y),
                    true => OutputColor::BLACK,
                };

                self.output.set(x * 2 + 0, y * 2 + 0, color);
                self.output.set(x * 2 + 1, y * 2 + 0, color);
                self.output.set(x * 2 + 0, y * 2 + 1, color);
                self.output.set(x * 2 + 1, y * 2 + 1, color);
            }

            if self.hpos == 277 && self.vpos == height {
                return true;
            }
        }

        false
    }

    fn render_pixel(&self, io: &PpuIo, x: u16, y: u16) -> OutputColor {
        let mode = io.backgrounds.mode.value();
        if mode == 7 {
            todo!()
        }
        let mode_def = &ModeDefinition::MODES[usize::from(mode)];

        let window = self.compute_window_mask(io, x);

        let colors = self.get_layer_colors(io, x, y, mode_def);
        let main_layers = io.screens.tm & !(window & io.windows.tmw);
        let sub_layers = io.screens.ts & !(window & io.windows.tsw);

        fn select_color(
            colors: &[LayerColor; NUM_LAYERS],
            mut layers: u8,
            bg3_high_priority: bool,
        ) -> (Color, u8) {
            if bg3_high_priority
                && (layers & (1 << LAYER_BG3) != 0)
                && colors[LAYER_BG3 as usize].priority > 0
            {
                return (colors[LAYER_BG3 as usize].color, LAYER_BG3);
            }

            layers &= 0x1F;

            let mut layer = LAYER_BACKDROP;
            while layers != 0 {
                let i = layers.trailing_zeros() as u8;
                layers &= layers - 1;
                if colors[i as usize].priority > colors[layer as usize].priority {
                    layer = i;
                }
            }
            (colors[layer as usize].color, layer)
        }

        let bg3_high_priority = mode == 1 && io.backgrounds.bg3_high_priority;
        let (mut main_color, main_layer) = select_color(&colors, main_layers, bg3_high_priority);

        let window_math_enabled = (window & WINDOW_MATH) == 0;
        let enable_screen_lut = [false, window_math_enabled, !window_math_enabled, true];

        let enable_main_screen = enable_screen_lut[usize::from(io.windows.main_screen_black as u8)];

        if !enable_main_screen {
            main_color = Color::BLACK;
        }

        let math_enabled = match main_layer {
            LAYER_BG1 => io.screens.math_on_backgrounds[0],
            LAYER_BG2 => io.screens.math_on_backgrounds[1],
            LAYER_BG3 => io.screens.math_on_backgrounds[2],
            LAYER_BG4 => io.screens.math_on_backgrounds[3],
            LAYER_OBJ => io.screens.math_on_objects,
            _ => io.screens.math_on_backdrop,
        };

        if !math_enabled {
            return OutputColor::from_rgb(main_color.r, main_color.g, main_color.b);
        }

        let enable_sub_screen = enable_screen_lut[usize::from(io.windows.sub_screen_black as u8)];

        let mut sub_color = Color::BLACK;
        let mut sub_layer = LAYER_BACKDROP;
        if enable_sub_screen {
            (sub_color, sub_layer) = match io.screens.sub_screen_bg_obj_enable {
                true => select_color(&colors, sub_layers, bg3_high_priority),
                false => (
                    Color::new(
                        io.screens.backdrop_red,
                        io.screens.backdrop_green,
                        io.screens.backdrop_blue,
                    ),
                    0xFF,
                ),
            };
        }

        let mut output = [
            sub_color.r.value() as i8,
            sub_color.g.value() as i8,
            sub_color.b.value() as i8,
        ];

        if io.screens.math_operation == MathOperation::Sub {
            output.map(std::ops::Neg::neg);
        }

        output[0] += main_color.r.value() as i8;
        output[1] += main_color.g.value() as i8;
        output[2] += main_color.b.value() as i8;

        if io.screens.half && enable_main_screen && sub_layer != LAYER_BACKDROP {
            output = output.map(|v| v / 2);
        }

        output = output.map(|v| v.clamp(0x00, 0x1F));

        OutputColor::from_rgb(
            u5::extract_u8(output[0] as u8, 0),
            u5::extract_u8(output[1] as u8, 0),
            u5::extract_u8(output[2] as u8, 0),
        )
    }

    fn compute_window_mask(&self, io: &PpuIo, x: u16) -> u8 {
        let pos = (x >> io.setini_hpseudo512 as u8) as u8;
        let outside_w1 = pos < io.windows.window1_left || pos > io.windows.window1_right;
        let outside_w2 = pos < io.windows.window2_left || pos > io.windows.window2_right;

        let mut w1 = (outside_w1 as u8).wrapping_sub(1);
        let mut w2 = (outside_w2 as u8).wrapping_sub(1);

        w1 &= io.windows.w1en;
        w2 &= io.windows.w2en;

        w1 ^= io.windows.w1inv;
        w2 ^= io.windows.w2inv;

        let or = w1 | w2;
        let and = w1 & w2;
        let xor = w1 ^ w2;
        let xnor = !xor;

        // PERF: pre-compute these masks
        let mut masks = [0; 4];
        masks[io.windows.backgrounds[0] as usize] |= WINDOW_BG1;
        masks[io.windows.backgrounds[1] as usize] |= WINDOW_BG2;
        masks[io.windows.backgrounds[2] as usize] |= WINDOW_BG3;
        masks[io.windows.backgrounds[3] as usize] |= WINDOW_BG4;
        masks[io.windows.objects as usize] |= WINDOW_OBJ;
        masks[io.windows.math as usize] |= WINDOW_MATH;

        (or & masks[0]) | (and & masks[1]) | (xor & masks[2]) | (xnor & masks[3])
    }

    fn get_layer_colors(
        &self,
        io: &PpuIo,
        x: u16,
        y: u16,
        mode_def: &ModeDefinition,
    ) -> [LayerColor; NUM_LAYERS] {
        let mut colors = [LayerColor::TRANSPARENT; NUM_LAYERS];
        colors[LAYER_BACKDROP as usize] = LayerColor::new(self.get_color(io, 0), 0);

        for i in 0..usize::from(mode_def.num_backgrounds) {
            colors[i] = self.get_bg_color(io, x, y, i, mode_def);
        }

        colors
    }

    #[inline(never)]
    fn get_bg_color(
        &self,
        io: &PpuIo,
        x: u16,
        y: u16,
        bg_num: usize,
        mode_def: &ModeDefinition,
    ) -> LayerColor {
        let bg = &io.backgrounds.backgrounds[bg_num];

        // screens in the order: top left, top right, bottom left, bottom right
        let screens: [u8; 4] =
            [[0, 0, 0, 0], [0, 1, 0, 1], [0, 0, 1, 1], [0, 1, 2, 3]][bg.size as usize];

        let tile_size = 8 << (bg.large_tiles as u8);

        let translated_x = x.wrapping_add(bg.h_offset & 0x3FF);
        let translated_y = y.wrapping_add(bg.v_offset & 0x3FF);

        let tile_x = (translated_x / tile_size) & 0x3F;
        let tile_y = (translated_y / tile_size) & 0x3F;
        let tile_off_x = translated_x % tile_size;
        let tile_off_y = translated_y % tile_size;

        let quadrant = (tile_x >> 5) | (tile_y >> 4 & 0x02);
        let screen = screens[usize::from(quadrant)];

        let tile_idx = (tile_y & 0x1F) * 32 + (tile_x & 0x1F);

        let bpp = mode_def.bpp[bg_num] as u16;
        let palette_offset = mode_def.palette_offset[bg_num];
        self.get_screen_color(
            io,
            bg,
            screen,
            tile_idx,
            tile_off_x,
            tile_off_y,
            bpp,
            palette_offset,
            &mode_def.bg_priorities[bg_num],
        )
    }

    fn get_screen_color(
        &self,
        io: &PpuIo,
        bg: &Background,
        screen: u8,
        tile_idx: u16,
        mut tile_off_x: u16,
        mut tile_off_y: u16,
        bpp: u16,
        palette_offset: u8,
        priorities: &[u8; 2],
    ) -> LayerColor {
        let tilemap_addr = ((bg.base_address.value() + screen) as u16) << 10; // * 1024
        let map_entry_addr = tilemap_addr.wrapping_add(tile_idx) << 1;
        let map_entry_lo = io.vram[usize::from(map_entry_addr + 0)];
        let map_entry_hi = io.vram[usize::from(map_entry_addr + 1)];
        let map_entry = (map_entry_lo as u16) | (map_entry_hi as u16) << 8;

        let tile_size = 8 << (bg.large_tiles as u8);

        let mut tile_number = map_entry & 0x03FF;
        let palette_number = ((map_entry >> 10) & 0x7) as u8;
        let bg_priority = (map_entry >> 13) & 1 != 0;
        let x_flip = (map_entry >> 14) & 1 != 0;
        let y_flip = (map_entry >> 15) & 1 != 0;

        if x_flip {
            tile_off_x = tile_size - 1 - tile_off_x;
        }
        if y_flip {
            tile_off_y = tile_size - 1 - tile_off_y;
        }

        tile_number = tile_number.wrapping_add(tile_off_x >> 3);
        tile_number = tile_number.wrapping_add((tile_off_y >> 3) * 16);

        let bytes_per_tile = bpp * 8;

        let tiles_addr = (bg.tile_base_address.value() as u16) << 13; // * 8192
        let tile_addr = tiles_addr.wrapping_add(tile_number * bytes_per_tile);

        let mut palette_idx = 0;

        for plane_off in (0..bpp).step_by(2) {
            let plane_pair_addr = tile_addr
                .wrapping_add((tile_off_y & 0x07) * 2)
                .wrapping_add(plane_off * 8);
            let plane1 = io.vram[usize::from(plane_pair_addr) + 0];
            let plane2 = io.vram[usize::from(plane_pair_addr) + 1];

            let bit1 = plane1.rotate_left(tile_off_x as u32 + 1) & 1;
            let bit2 = plane2.rotate_left(tile_off_x as u32 + 1) & 1;

            palette_idx |= (bit1 | bit2 << 1) << plane_off;
        }

        if palette_idx == 0 {
            return LayerColor::TRANSPARENT;
        }

        if bpp < 8 {
            palette_idx += palette_number << bpp;
        }
        palette_idx += palette_offset;
        LayerColor::new(
            self.get_color(io, palette_idx),
            priorities[bg_priority as usize],
        )
    }

    fn get_color(&self, io: &PpuIo, palette_idx: u8) -> Color {
        let cgram_addr = usize::from(palette_idx) * 2;
        let color_lo = io.cgram[cgram_addr];
        let color_hi = io.cgram[cgram_addr + 1];
        let color = (color_lo as u16) | (color_hi as u16) << 8;

        let r = u5::extract_u16(color, 0);
        let g = u5::extract_u16(color, 5);
        let b = u5::extract_u16(color, 10);

        Color::new(r, g, b)
    }
}

struct ModeDefinition {
    num_backgrounds: u8,
    bpp: [u8; 4],
    palette_offset: [u8; 4],
    bg_priorities: [[u8; 2]; 4],
    obj_priorities: [u8; 4],
}

impl ModeDefinition {
    const MODE0: Self = Self {
        num_backgrounds: 4,
        bpp: [2, 2, 2, 2],
        palette_offset: [0, 32, 64, 96],
        bg_priorities: [[8, 11], [7, 10], [2, 5], [1, 4]],
        obj_priorities: [3, 6, 9, 12],
    };
    const MODE1: Self = Self {
        num_backgrounds: 3,
        bpp: [4, 4, 2, 0],
        palette_offset: [0, 0, 0, 0],
        bg_priorities: [[8, 11], [7, 10], [2, 5], [0, 0]],
        obj_priorities: [3, 6, 9, 12],
    };
    const MODE2: Self = Self {
        num_backgrounds: 2,
        bpp: [4, 4, 0, 0],
        palette_offset: [0, 0, 0, 0],
        bg_priorities: [[3, 7], [1, 5], [0, 0], [0, 0]],
        obj_priorities: [2, 4, 6, 8],
    };
    const MODE3: Self = Self {
        num_backgrounds: 2,
        bpp: [8, 4, 0, 0],
        palette_offset: [0, 0, 0, 0],
        bg_priorities: [[3, 7], [1, 5], [0, 0], [0, 0]],
        obj_priorities: [2, 4, 6, 8],
    };
    const MODE4: Self = Self {
        num_backgrounds: 2,
        bpp: [8, 2, 0, 0],
        palette_offset: [0, 0, 0, 0],
        bg_priorities: [[3, 7], [1, 5], [0, 0], [0, 0]],
        obj_priorities: [2, 4, 6, 8],
    };
    const MODE5: Self = Self {
        num_backgrounds: 2,
        bpp: [4, 2, 0, 0],
        palette_offset: [0, 0, 0, 0],
        bg_priorities: [[3, 7], [1, 5], [0, 0], [0, 0]],
        obj_priorities: [2, 4, 6, 8],
    };
    const MODE6: Self = Self {
        num_backgrounds: 1,
        bpp: [4, 0, 0, 0],
        palette_offset: [0, 0, 0, 0],
        bg_priorities: [[2, 5], [0, 0], [0, 0], [0, 0]],
        obj_priorities: [1, 3, 4, 6],
    };

    const MODES: [Self; 7] = [
        Self::MODE0,
        Self::MODE1,
        Self::MODE2,
        Self::MODE3,
        Self::MODE4,
        Self::MODE5,
        Self::MODE6,
    ];
}

#[derive(Default, Clone, Copy)]
struct LayerColor {
    color: Color,
    priority: u8,
}

impl LayerColor {
    const TRANSPARENT: Self = Self::new(Color::BLACK, 0);

    const fn new(color: Color, priority: u8) -> Self {
        Self { color, priority }
    }
}

#[derive(Default, Clone, Copy)]
struct Color {
    r: u5,
    g: u5,
    b: u5,
}

impl Color {
    const BLACK: Self = Self::new(u5::new(0), u5::new(0), u5::new(0));

    const fn new(r: u5, g: u5, b: u5) -> Self {
        Self { r, g, b }
    }
}
