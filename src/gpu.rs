//
//      Graphics Processing Unit
//
#[allow(dead_code)]

use cpu::Interrupt;

use piston::input;
use piston_window::*;
use graphics::types::SourceRectangle;

const VRAM_SIZE: usize = 0x2000;
pub const OAM_SIZE: usize = 0x9F;   // 0xfe00 - 0xfe9f is OAM
const OAM_ENTRY_SIZE: usize = 4;
const OBJ_COUNT: usize =  40;    // sprite count
const NUM_TILES: usize = 192;       // number of in-memory tiles

pub const HEIGHT: usize = 144;
pub const WIDTH: usize = 160;

pub type ScreenData = [u8; WIDTH * HEIGHT * 4];
pub type Color = [u8; 4];
pub type Palette = [Color; 4];

struct Palettes {
    bg: Palette,
    obp0: Palette,
    obp1: Palette,
}

const PALETTE_BW: Palette = [
    [255, 255, 255, 255],
    [148, 148, 148, 255],
    [ 86,  86,  86, 255],
    [  0,   0,   0, 255],
];
const PALETTE_GREEN: Palette = [
    [225, 247, 207, 255],
    [136, 193, 107, 255],
    [ 49,  106, 74, 255],
    [ 7,  24, 31, 255],
];
const PALETTE_PUKE_GREEN: Palette = [
    [157, 188, 7, 255],
    [122, 156, 107, 255],
    [ 53,  99, 56, 255],
    [ 13,  58, 8, 255],
];
// TODO: Switch palettes at runtome
const PALETTE: &'static Palette = &PALETTE_GREEN;

struct Tiles {
    data: [[[u8; 8]; 8]; NUM_TILES],
    need_update: bool,
    to_update: [bool; NUM_TILES],
}

#[derive(PartialEq, Eq, Debug, Copy, Clone)]
enum Mode {
    HBlank = 0x00, // mode 0
    VBlank = 0x01, // mode 1
    RdOam  = 0x02, // mode 2
    RdVram = 0x03, // mode 3
}

pub struct Gpu {
    pub oam: [u8; OAM_SIZE],

    pub image_data: Box<ScreenData>,

    pub is_cgb: bool,
    pub is_sgb: bool,
    c: u32,                                                      //remove
    d: u32,
    mode: Mode,

    pub clock: u32,

    pub vrambank: Box<[u8; VRAM_SIZE]>,

    // Selects vrambank (only 0 supported since we don't do CGB)
    vrambank_sel: u8,

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

/*
    mehcode [2:09 AM]
    Some notes
    - LY increments at the 0th cycle of every scanline
    - LY is reset to 0 on the 5th cycle of the 153rd (last) scanline
    - A scanline is exactly 456 T-cycles

    [2:10]
    Just a simple loop with a counter that ignores proper PPU operation should be able to match LY like that

    [2:10]
    But the comparison logic is tricky (LYC) (edited)

    [2:10]
    But then again I don’t think blargg uses that
*/
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
    pal: Box<Palettes>,

    // Compiled tiles
    tiles: Box<Tiles>,

    // Image for drawing
    pub img: Image,
}

impl Gpu {
    pub fn new<W: Window>(window: &PistonWindow<W>) -> Gpu {
        let mut gpu: Gpu = Gpu {
            image_data: Box::new([255; HEIGHT * WIDTH * 4]),
            oam: [0; OAM_SIZE],
            c:0,
            d:1,
            is_cgb: false,
            is_sgb: false,

            clock: 0,
            vrambank: Box::new([0; VRAM_SIZE]),
            vrambank_sel: 0,

            mode: Mode::RdOam,
            wx: 0, wy: 0, obp1: 0, obp0: 0, bgp: 0,
            lyc: 0, ly: 0, scx: 0, scy: 0,
            mode0int: false, mode1int: false, mode2int: false, lycly: false,
            bgon: false, objon: false, objsize: false, bgmap: false,
            tiledata: false,
            winon: false, winmap: false, lcdon: false,

            pal: Box::new(Palettes {
                bg: [[0; 4]; 4],
                obp0: [[0; 4]; 4],
                obp1: [[0; 4]; 4],
            }),

            tiles: Box::new(Tiles {
                need_update: true,  // Does this need to be true?
                to_update: [true;  NUM_TILES],
                data: [[[0; 8]; 8]; NUM_TILES],
            }),

            img: {
                let r: SourceRectangle = [0.0, 0.0, ::SCREEN_DIMS[0] as f64, ::SCREEN_DIMS[1] as f64];
                Image::new().src_rect(r)
            }
        };

        for i in 0..HEIGHT * WIDTH * 4 {
            gpu.image_data[(i) as usize] = 0; //rand::random();
        }

        // Is this needed?
        update_pal(&mut gpu.pal.bg, 0xE4);
        update_pal(&mut gpu.pal.obp0, 0xE4);
        update_pal(&mut gpu.pal.obp1, 0xE4);

        // BIOS SKIP
        gpu.clock = 0xABCC % 456;

        // for y in 0..HEIGHT {
        //     for x in 0..WIDTH {
        //         gpu.image_data[((y * WIDTH) + x) as usize] = PALETTE[2];
        //     }
        // }
        gpu
    }

    pub fn display<W: Window>(&mut self, window: &mut PistonWindow<W>, evt: &input::Event) {
        //self.update();

        // window.draw_2d(&evt, |c, g| {
        //     self.img.draw(&framebuffer, &c.draw_state, c.transform, g);
        // })
        //framebuffer = from_memory(&mut window.factory, &*emu.mem.gpu.image_data, 160, 144, &ts).unwrap();

        // window.draw_2d(evt, |c, g| {
        //     for y in 0..HEIGHT {
        //         for x in 0..WIDTH {

        //         }
        //     }
        // });
    }

    pub fn update(&mut self) {

        // Debug code

        // Randomize one random pixel
        //let x = (rand::random::<u64>() % 160) as usize;
        //let y = (rand::random::<u64>() % 144) as usize;
        //self.set_pixel(x, y, 255, 255, 255);

        // Randomize every pixel
        // for i in 0..HEIGHT * WIDTH * 4 {
        //     self.image_data[(i) as usize] = rand::random();
        // }

        // self.clock += 1;
    }

    pub fn rb_vram(&self, addr: u16) -> u8 {
        match addr {
            0x8000 ... 0x9FFF => self.vrambank[addr as usize - 0x8000],
            //0xA000 ... 0xBFFF => self.vrambanks[1][addr as usize - 0xA000],
            _ => unreachable!()
        }
    }

    pub fn wb_vram(&mut self, addr: u16, data: u8) {
        match addr {
            0x8000 ... 0x9FFF => {
                //trace!("writing to VRAM1 {:04X}  data {:02X}", addr - 0x8000, data);
                let mut tilei: u16;

                tilei = (addr - 0x8000 as u16) / 16;

                if tilei < NUM_TILES as u16 {
                    self.tiles.to_update[tilei as usize] = true;
                    self.tiles.need_update = true;
                }

                if !self.tiledata && addr >= 0x8800 {
                    tilei = (addr - 0x8800 as u16) / 16;
                    if tilei < NUM_TILES as u16 {
                        self.tiles.to_update[tilei as usize] = true;
                        self.tiles.need_update = true;
                    }
                }
                self.vrambank[addr as usize - 0x8000] = data;
            },
            // 0xA000 ... 0xBFFF => {
            //    //trace!("writing to VRAM2 {:04X}  data {:02X}", addr - 0xA000 , data);
            //    self.vrambanks[1][addr as usize - 0xA000] = data;
            // }
            _ => unreachable!()
        }
    }

    pub fn rb(&self, addr: u16) -> u8 {
        match addr & 0xff {
            0x40 => {
                warn!("BG read {}",self.bgon);
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
            0x4f => self.vrambank_sel,

            _ => 0xff
        }
    }

    pub fn wb(&mut self, addr: u16, val: u8) {
        match addr & 0xff {
            0x40 => {
                warn!("BG write {}  {:02X}",self.bgon, val);
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
            0x47 => { self.bgp = val; update_pal(&mut self.pal.bg, val); }
            0x48 => { self.obp0 = val; update_pal(&mut self.pal.obp0, val); }
            0x49 => { self.obp1 = val; update_pal(&mut self.pal.obp1, val); }
            0x4a => { self.wy = val; }
            0x4b => { self.wx = val; }
            0x4f => { if self.is_cgb { self.vrambank_sel = val & 1; } }

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
        //      http://http://problemkaputt.de//pandocs.htm#lcdstatusregister
        self.clock += clocks;

        // If clock >= 456, then we've completed an entire line. This line might
        // have been part of a vblank or part of a scanline.
        if self.clock >= 456 {
            self.clock -= 456;
            self.ly = (self.ly + 1) % 154; // 144 lines tall, 10 for a vblank

            // debug!("Completed an entire line");

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
                trace!("HBlank! Rendering...");
                self.render_line();
                if self.mode0int {
                    *if_ |= Interrupt::LCDStat as u8;
                }
            }
            Mode::VBlank => {
                // TODO: a frame is ready, it should be put on screen at this
                // point
                debug!("GPU: VBlank!");
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

    fn update_tileset(&mut self) {

        let tiles = &mut *self.tiles;
        let iter = tiles.to_update.iter_mut();
        info!("Updating tileset... Tiles: {}", iter.len());

        for (i, slot) in iter.enumerate().filter(|&(_, &mut i)| i) {
            *slot = false;

            // Each tile is 16 bytes long. Each pair of bytes represents a line
            // of pixels (making 8 lines). The first byte is the LSB of the
            // color number and the second byte is the MSB of the color.
            //
            // For example, for:
            //      byte 0 : 00011011
            //      byte 1 : 01101010
            //
            // The colors are [0, 2, 2, 1, 3, 0, 3, 1]
            // println!("-- memory addr: {:#0X}   {:#0X}", ((i % NUM_TILES) * 16) + 0x8000, self.vrambank[((i % NUM_TILES) * 16)]);
            for j in 0..8 {
                let addr = ((i % NUM_TILES) * 16) + j * 2;

                // println!("memory addr: {:#0X}", addr + 0x8000);
                // All tiles are located 0x8000-0x97ff => 0x0000-0x17ff in VRAM
                // meaning that the index is simply an index into raw VRAM
                let (mut lsb, mut msb) = if i < NUM_TILES {
                    (self.vrambank[addr], self.vrambank[addr + 1])
                } else {
                    panic!("second VRAM bank used");
                    //(self.vrambanks[1][addr], self.vrambanks[1][addr + 1])
                };

                // LSB is the right-most pixel.
                for k in (0..8).rev() {
                    tiles.data[i][j][k] = ((msb & 1) << 1) | (lsb & 1);
                    // println!("lsb {:#08b} msb {:#08b} tiledata {:#02X}", lsb, msb, tiles.data[i][j][k]);
                    lsb >>= 1;
                    msb >>= 1;
                }
            }

            //debug!("{:?}\t{:?}", i, tiles.data[i]);
        }
    }

    pub fn bgbase(&self) -> usize {
        // vram is from 0x8000-0x9fff
        // self.bgmap: 0=9800-9bff, 1=9c00-9fff
        //
        // Each map is a 32x32 (1024) array of bytes. Each byte is an index into
        // the tile map. Each tile is an 8x8 block of pixels.
        if self.bgmap {0x1c00} else {0x1800}
    }

    fn render_line(&mut self) {
        if !self.lcdon { return }

        let mut scanline = [0u8; WIDTH];

        if self.tiles.need_update {
            self.update_tileset();
            self.tiles.need_update = false;
        }

        if self.bgon {
            self.render_background(&mut scanline);
        }
        if self.winon {
            //self.render_window(&mut scanline);
        }
        if self.objon {
            self.render_sprites(&mut scanline);
        }
    }

    pub fn add_tilei(&self, base: usize, tilei: u8) -> usize {
        // tiledata = 0 => tilei is a signed byte, so fix it here
        if self.tiledata {
            base + tilei as usize
        } else {
            //base + (tilei as isize + 128 as isize) as usize
            (base as isize + (tilei as i8 as isize)) as usize
        }
    }

    fn render_background(&mut self, scanline: &mut [u8; WIDTH]) {
        let mapbase = self.bgbase();
        let line = self.ly as usize + self.scy as usize;

        // Now offset from the base to the right location. We divide by 8
        // because each tile is 8 pixels high. We then multiply by 32
        // because each row is 32 bytes long. We can't just multiply by 4
        // because we need the truncation to happen beforehand
        let mapbase = mapbase + (((line % 256) >> 3) << 5);

        // X and Y location inside the tile itself to paint
        let y = (self.ly.wrapping_add(self.scy)) % 8;
        let mut x = self.scx % 8;

        // Offset into the canvas to draw. line * width * 4 colors
        let mut coff = (self.ly as usize) * WIDTH * 4;

        let mut i = 0;
        // this.tiledata is a flag to determine which tile data table to use
        // 0=8800-97FF, 1=8000-8FFF. For some odd reason, if tiledata = 0, then
        // (&tiles[0]) == 0x9000, where if tiledata = 1, (&tiles[0]) = 0x8000.
        // This implies that the indices are treated as signed numbers.
        // TODO: should this be 0x800 ?
        let tilebase = if !self.tiledata {256} else {0};

        //info!("render background. mapbase:{:x} scx:{} scy:{}", mapbase, self.scx, self.scy);

        // let mut bg_tiles = [0u8; 20];
        // let mut loop_c: usize = 0;
        loop {
            // Backgrounds wrap around, so calculate the offset into the bgmap
            // each loop to check for wrapping
            let mapoff = ((i as usize + self.scx as usize) % 256) >> 3;
            let tilei = self.vrambank[mapbase + mapoff];
            // bg_tiles[loop_c] = tilei;
            // tiledata = 0 => tilei is a signed byte, so fix it here
            let tilebase = tilei%192;//self.add_tilei(tilebase, tilei);
            // println!("tilebase: {}", tilebase);

            let row;
            let bgpri;
            let hflip;
            let bgp;

            row = self.tiles.data[tilebase as usize][y as usize];
            bgpri = false;
            hflip = false;
            bgp = self.pal.bg;

            while x < 8 && i < WIDTH as u8 {
                let colori = row[if hflip {7 - x} else {x} as usize];

                // To indicate bg priority, list a color >= 4
                scanline[i as usize] = if bgpri {4} else {colori};

                set_pixel_index(&mut self.image_data, coff, colori as usize, &bgp);

                x += 1;
                i += 1;
                coff += 4;
            }

            x = 0;
            // loop_c += 1;
            if i >= WIDTH as u8 { break }
        }

        // Dump bg tiles
        // println!("LINE: {:03} | LY: {:03} | {:?}", line, self.ly, bg_tiles);
    }

    fn render_window(&mut self, scanline: &mut [u8; WIDTH]) {
        // TODO: Window rendering
    }

    fn render_sprites(&mut self, scanline: &mut [u8; WIDTH]) {
        let line = self.ly as i32;
        let ysize = if self.objsize {16} else {8};

        // All sprits are located in OAM
        // There are 40 sprites in total, each is 4 bytes wide
        for sprite in self.oam.chunks(4) {
            let mut yoff = (sprite[0] as i32) - 16;
            let xoff = (sprite[1] as i32) - 8;
            let mut tile = sprite[2] as usize;
            let flags = sprite[3];

            // First make sure that this sprite even lands on the current line
            // being rendered. The y value in the sprite is the top left corner,
            // so if that is below the scanline or the bottom of the sprite
            // (which is 8 pixels high) lands below the scanline, this sprite
            // doesn't need to be rendered right now
            if yoff > line || yoff + ysize <= line ||
               xoff <= -8 || xoff >= WIDTH as i32 {
               continue
            }

            // 8x16 tiles always use adjacent tile indices. If we're in 8x16
            // mode and this sprite needs the second tile, add 1 to the tile
            // index and change yoff so it looks like we're rendering that tile
            if ysize == 16 {
                tile &= 0xfe; // ignore the lowest bit
                if line - yoff >= 8 {
                    tile |= 1;
                    yoff += 8;
                }
            }

            // 160px/line, 4 entries/px
            let mut coff = (WIDTH as i32 * line + xoff) * 4;

            // All sprite tile palettes are at 0x8000-0x8fff => start of vram.
            // If we're in CGB mode, then we get our palette from the spite
            // flags. We also need to take into account the tile being in a
            // different bank. Otherwise, we just use the tile index as a raw
            // index.
            // bit4 is the palette number. 0 = obp0, 1 = obp1
           let pal = if flags & 0x10 != 0 {self.pal.obp1} else {self.pal.obp0};
           let tiled = self.tiles.data[tile as usize];


            // bit6 is the vertical flip bit
            let row = if flags & 0x40 != 0 {
                tiled[(7 - (line - yoff)) as usize]
            } else {
                tiled[(line - yoff) as usize]
            };

            for x in 0..8 {
                coff += 4;

                // If these pixels are off screen, don't bother drawing
                // anything. Also, if the background tile at this pixel has
                // priority, don't render this sprite at all.
                if xoff + x < 0 || xoff + x >= WIDTH as i32 ||
                   scanline[(x + xoff) as usize] > 3 {
                    continue
                }
                // bit5 is the horizontal flip flag
                let colori = row[if flags & 0x20 != 0 {7-x} else {x} as usize];

                // A color index of 0 for sprites means transparent
                if colori == 0 { continue }

                // bit7 0=OBJ Above BG, 1=OBJ Behind BG color 1-3. So if this
                // sprite has this flag set and the data at this location
                // already contains data (nonzero), then don't render this
                // sprite
                if flags & 0x80 != 0 && scanline[(xoff + x) as usize] != 0 {
                    continue
                }

                let color = pal[colori as usize];

                let palette = if flags & 0x10 != 0 {self.pal.obp0} else {self.pal.obp1};
                set_pixel_index(&mut self.image_data, coff as usize - 4, colori as usize, &palette);
            }
        }
    }

    pub fn dump_tiles(&self) {
        use image::{ImageBuffer, RgbaImage, Rgba};

        static TILE_SIZE_X: u32 = 16 * 8;
        static TILE_SIZE_Y: u32 = 12 * 8;

        let mut img: RgbaImage = ImageBuffer::new(TILE_SIZE_X, TILE_SIZE_Y);

        for y in 0..TILE_SIZE_Y as usize {
            for x in 0..TILE_SIZE_X as usize {
                let tilei_x = x / 8;
                let tilei_y = y / 8;
                let tilei = tilei_x + 16 * tilei_y;

                let tile = self.tiles.data[tilei];

                let colori = tile[y % 8][x % 8];

                let r = PALETTE[colori as usize][0];
                let g = PALETTE[colori as usize][1];
                let b = PALETTE[colori as usize][2];

                img.put_pixel(x as u32, y as u32, Rgba { data: [r, g, b, 255]})
            }
        }

        img.save("tile_dump.png").unwrap();
        info!("Tiles dumped to tile_dump.png");
    }
}


#[inline]
fn set_pixel(image_data: &mut ScreenData, x: usize, y: usize, r: u8, g: u8, b: u8) {
    let first_byte = 4 * (x + (y * 160)) as usize;

    image_data[first_byte] = r;      // R
    image_data[first_byte+1] = g;    // G
    image_data[first_byte+2] = b;    // B
    image_data[first_byte+3] = 255;  // A
}

#[inline]
fn set_pixel_index(image_data: &mut ScreenData, first_byte: usize, colori: usize, pal: &Palette) {
    image_data[first_byte] = pal[colori][0];    // R
    image_data[first_byte+1] = pal[colori][1];  // G
    image_data[first_byte+2] = pal[colori][2];  // B
    image_data[first_byte+3] = pal[colori][0];  // A
}

// Update the cached palettes for BG/OBP0/OBP1. This should be called whenever
// these registers are modified
fn update_pal(pal: &mut Palette, val: u8) {
    // These registers are indices into the actual palette. See
    // http://problemkaputt.de/pandocs.htm#lcdmonochromepalettes
    pal[0] = PALETTE[((val >> 0) & 0x3) as usize];
    pal[1] = PALETTE[((val >> 2) & 0x3) as usize];
    pal[2] = PALETTE[((val >> 4) & 0x3) as usize];
    pal[3] = PALETTE[((val >> 6) & 0x3) as usize];
    info!("BG Color: {:?} val {:02X}", pal, val);
}