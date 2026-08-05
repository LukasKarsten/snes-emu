# SNES Emulator

A Super Nintendo Entertainment System emulator running natively and in the
browser.

> [!IMPORTANT]
> This project is very much a work in progress. Expect incomplete emulation and
> bugs.

[Try it out in your browser!](https://lukaskarsten.de/snes-emu)

## Screenshots

![A 2 by 2 grid of screenshots showing gameplay from different SNES titles across various genres.](https://static.lukaskarsten.de/snes-emu.png)

## Button mappings

| SNES Gamepad | Host Keyboard |
|--------------|---------------|
| Start        | Escape        |
| Select       | Space         |
| Up           | W             |
| Down         | S             |
| Left         | A             |
| Right        | D             |
| A            | L             |
| B            | K             |
| X            | I             |
| Y            | J             |
| L            | U             |
| R            | O             |

## Building from source

1. Ensure you have [Rust](https://rust-lang.org/) 1.92.0 or newer installed.

2. Clone the repository:
   ```sh
   git clone https://codeberg.org/LukasKarsten/snes-emu.git
   cd snes-emu
   ```

3. Build the binary:
   ```sh
   cargo build --release
   ```
   After the build is done, the executable can be found in `target/release/`.
