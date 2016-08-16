//
//		Graphics Processing Unit
//
use std::mem;
use std::iter;

use cpu::Interrupt;

use piston::input;
use piston_window;
use piston_window::*;
use gfx_core::Resources;

const VRAM_SIZE: usize = 8 << 10; // 8K
const OAM_SIZE: usize = 0xa0;     // 0xffe00 - 0xffe9f is OAM
const CGB_BP_SIZE: usize = 64;    // 64 bytes of extra memory
const NUM_TILES: usize = 192;     // number of in-memory tiles

pub const HEIGHT: usize = 144;
pub const WIDTH: usize = 160;

pub type Color = [u8; 4];

const PIXEL_COLOR: Color = [40, 88, 200, 255];

// Palette for the monochrome GB. Possible values are:
//
// 0 - white
// 1 - light gray
// 2 - dark gray
// 3 - black
const PALETTE: [Color; 4] = [
    [255, 255, 255, 255],
    [192, 192, 192, 255],
    [ 96,  96,  96, 255],
    [  0,   0,   0, 255],
];

struct Tiles {
    data: [[[u8; 8]; 8]; NUM_TILES * 2],
    need_update: bool,
    to_update: [bool; NUM_TILES * 2],
}

pub struct Gpu {
    pub oam: [u8; OAM_SIZE],

    image_data: Box<[u8; WIDTH * HEIGHT * 4]>,

    pub is_cgb: bool,
    pub is_sgb: bool,

    mode: Mode,

    clock: u32,

    // 0xff40 - LCD control (LCDC) - in order from most to least significant bit
    pub lcdon: bool,    // LCD monitor turned on or off?
        winmap: bool,   // Window Tile Map Display (0=9800-9BFF, 1=9C00-9FFF)
        winon: bool,    // Window Display Enable   (0=Off, 1=On)
    pub tiledata: bool, // BG & Window Tile Data   (0=8800-97FF, 1=8000-8FFF)
        bgmap: bool,    // BG Tile Map Display     (0=9800-9BFF, 1=9C00-9FFF)
        objsize: bool,  // OBJ (Sprite) Size       (0=8x8, 1=8x16)
        objon: bool,    // OBJ (Sprite) Display    (0=Off, 1=On)
        bgon: bool,     // BG Display              (0=Off, 1=On)

    // 0xff41 - STAT - LCDC Status - starts with bit 6
    lycly: bool,    // LYC=LY Coincidence Interrupt (1=Enable)
    mode2int: bool, // Mode 2 OAM Interrupt         (1=Enable)
    mode1int: bool, // Mode 1 V-Blank Interrupt     (1=Enable)
    mode0int: bool, // Mode 0 H-Blank Interrupt     (1=Enable)

    // 0xff42 - SCY - Scroll Y
    scy: u8,
    // 0xff43 - SCX - Scroll X
    scx: u8,
    // 0xff44 - LY - LCDC Y-Coordinate
    ly: u8,
    // 0xff45 - LYC - LY Compare
    lyc: u8,

    // 0xff47 - BGP - BG Palette Data
    bgp: u8,
    // 0xff48 - OBP0 - Object Palette 0 Data
    obp0: u8,
    // 0xff49 - OBP1 - Object Palette 1Data
    obp1: u8,
    // 0xff4a - WY - Window Y Position
    wy: u8,
    // 0xff4b - WX - Window X Position minus 7
    wx: u8,

    // Compiled palettes. These are updated when writing to BGP/OBP0/OBP1. Meant
    // for non CGB use only. Each palette is an array of 4 color schemes. Each
    // color scheme is one in PALETTE.
    pal: Box<Palette>,

    // Compiled tiles
    tiles: Box<Tiles>,
}

impl Gpu {
    pub fn new(window: &PistonWindow) -> Gpu {
        let mut gpu: Gpu = Gpu {
            image_data: Box::new([255; HEIGHT * WIDTH * 4]),
            oam: [0; OAM_SIZE],

            is_cgb: false,
            is_sgb: false,
            
            clock: 0,
            
            mode: Mode::RdOam,
            wx: 0, wy: 0, obp1: 0, obp0: 0, bgp: 0,
            lyc: 0, ly: 0, scx: 0, scy: 0,
            mode0int: false, mode1int: false, mode2int: false, lycly: false,
            bgon: false, objon: false, objsize: false, bgmap: false,
            tiledata: false,
            winon: false, winmap: false, lcdon: false,

            pal: Box::new(Palette {
                bg: [[0; 4]; 4],
                obp0: [[0; 4]; 4],
                obp1: [[0; 4]; 4],
            }),

            tiles: Box::new(Tiles {
                need_update: false,
                to_update: [false;  NUM_TILES * 2],
                data: [[[0; 8]; 8]; NUM_TILES * 2],
            }),


        };
        gpu
    }

    pub fn display(&mut self, window: &mut piston_window::PistonWindow, evt: &input::Event) {
        self.update();

        window.draw_2d(evt, |c, g| {
            for y in 0..HEIGHT {
                for x in 0..WIDTH {
                    
                }
            }
        });
    }

    fn update(&mut self) {

    }

    pub fn rb(&self, addr: u16) -> u8 {
        match addr & 0xff {
            0x40 => {
                ((self.lcdon as u8)    << 7) |
                ((self.winmap as u8)   << 6) |
                ((self.winon as u8)    << 5) |
                ((self.tiledata as u8) << 4) |
                ((self.bgmap as u8)    << 3) |
                ((self.objsize as u8)  << 2) |
                ((self.objon as u8)    << 1) |
                ((self.bgon as u8)     << 0)
            }

            0x41 => {
                ((self.lycly as u8)                                   << 6) |
                ((self.mode2int as u8)                                << 5) |
                ((self.mode1int as u8)                                << 4) |
                ((self.mode0int as u8)                                << 3) |
                ((if self.lycly as u8 == self.ly {1} else {0} as u8) << 2) |
                ((self.mode as u8)                                    << 0)
            }

            0x42 => self.scy,
            0x43 => self.scx,
            0x44 => self.ly,
            0x45 => self.lyc,
            // 0x46 is DMA transfer, can't read
            0x47 => self.bgp,
            0x48 => self.obp0,
            0x49 => self.obp1,
            0x4a => self.wy,
            0x4b => self.wx,
            0x4f => {warn!("Tried to access vrambank (cgb only)"); 0xFF},

            _ => 0xff
        }
    }

    pub fn wb(&mut self, addr: u16, val: u8) {
        match addr & 0xff {
            0x40 => {
                let before = self.lcdon;
                self.lcdon    = (val >> 7) & 1 != 0;
                self.winmap   = (val >> 6) & 1 != 0;
                self.winon    = (val >> 5) & 1 != 0;
                self.tiledata = (val >> 4) & 1 != 0;
                self.bgmap    = (val >> 3) & 1 != 0;
                self.objsize  = (val >> 2) & 1 != 0;
                self.objon    = (val >> 1) & 1 != 0;
                self.bgon     = (val >> 0) & 1 != 0;
                if !before && self.lcdon {
                    self.clock = 4; // ??? why 4?!
                    self.ly = 0;
                }
            }

            0x41 => {
                self.lycly    = (val >> 6) & 1 != 0;
                self.mode2int = (val >> 5) & 1 != 0;
                self.mode1int = (val >> 4) & 1 != 0;
                self.mode0int = (val >> 3) & 1 != 0;
                // The other bits of this register are mode and lycly, but thse
                // are read-only and won't be modified
            }

            0x42 => { self.scy = val; }
            0x43 => { self.scx = val; }
            // 0x44 self.ly is read-only
            0x45 => { self.lyc = val; }
            // 0x46 handled in mem
            0x47 => { self.bgp = val; update_pal(&mut self.pal.bg, val); }
            0x48 => { self.obp0 = val; update_pal(&mut self.pal.obp0, val); }
            0x49 => { self.obp1 = val; update_pal(&mut self.pal.obp1, val); }
            0x4a => { self.wy = val; }
            0x4b => { self.wx = val; }

            // 0x55 handled in mem
            _ => {}
        }
    }

    // Step the GPU a number of clock cycles forward. The GPU's screen is
    // synchronized with the CPU clock because in a real GB, the two are
    // matched up on the same clock.
    //
    // This function mostly doesn't do anything except for incrementing its own
    // internal counter of clock cycles that have passed. It's a state machine
    // between a few different states. In one state, however, the rendering of a
    // screen occurs, but that doesn't always happen when calling this function.
    pub fn step(&mut self, clocks: u32, if_: &mut u8) {
        // Timings located here:
        //      http://nocash.emubase.de/pandocs.htm#lcdstatusregister
        self.clock += clocks;

        // If clock >= 456, then we've completed an entire line. This line might
        // have been part of a vblank or part of a scanline.
        if self.clock >= 456 {
            self.clock -= 456;
            self.ly = (self.ly + 1) % 154; // 144 lines tall, 10 for a vblank

            if self.ly >= 144 && self.mode != Mode::VBlank {
                self.switch(Mode::VBlank, if_);
            }

            if self.ly == self.lyc && self.lycly {
                *if_ |= Interrupt::LCDStat as u8;
            }
        }

        // Hop between modes if we're not in vblank
        if self.ly < 144 {
            if self.clock <= 80 { // RDOAM takes 80 cycles
                if self.mode != Mode::RdOam { self.switch(Mode::RdOam, if_); }
            } else if self.clock <= 252 { // RDVRAM takes 172 cycles
                if self.mode != Mode::RdVram { self.switch(Mode::RdVram, if_); }
            } else { // HBLANK takes rest of time before line rendered
                if self.mode != Mode::HBlank { self.switch(Mode::HBlank, if_); }
            }
        }
    }
    fn switch(&mut self, mode: Mode, if_: &mut u8) {
        self.mode = mode;
        match mode {
            Mode::HBlank => {
                //self.render_line();
                if self.mode0int {
                    *if_ |= Interrupt::LCDStat as u8;
                }
            }
            Mode::VBlank => {
                // TODO: a frame is ready, it should be put on screen at this
                // point
                *if_ |= Interrupt::Vblank as u8;
                if self.mode1int {
                    *if_ |= Interrupt::LCDStat as u8;
                }
            }
            Mode::RdOam => {
                if self.mode2int {
                    *if_ |= Interrupt::LCDStat as u8;
                }
            }
            Mode::RdVram => {}
        }
    }

}

#[derive(PartialEq, Eq, Debug, Copy, Clone)]
enum Mode {
    HBlank = 0x00, // mode 0
    VBlank = 0x01, // mode 1
    RdOam  = 0x02, // mode 2
    RdVram = 0x03, // mode 3
}

struct Palette {
    bg: [Color; 4],
    obp0: [Color; 4],
    obp1: [Color; 4],
}

// Update the cached palettes for BG/OBP0/OBP1. This should be called whenever
// these registers are modified
fn update_pal(pal: &mut [Color; 4], val: u8) {
    // These registers are indices into the actual palette. See
    // http://nocash.emubase.de/pandocs.htm#lcdmonochromepalettes
    pal[0] = PALETTE[((val >> 0) & 0x3) as usize];
    pal[1] = PALETTE[((val >> 2) & 0x3) as usize];
    pal[2] = PALETTE[((val >> 4) & 0x3) as usize];
    pal[3] = PALETTE[((val >> 6) & 0x3) as usize];
}