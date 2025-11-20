# dxf2elmt

dxf2elmt is a program that can convert .dxf files into [QElectroTech](https://qelectrotech.org/) .elmt files. The program supports both ASCII and binary .dxf files and is available as both a CLI tool and a desktop application with a graphical user interface.

The goal of this program is to create a fast and accurate conversion tool to be used with [QElectroTech](https://qelectrotech.org/).

## Repository

This desktop version is maintained at: [https://github.com/labeltec/dxf2elmt-desktop](https://github.com/labeltec/dxf2elmt-desktop)

The original CLI project is maintained at: [https://github.com/Vadoola/dxf2elmt](https://github.com/Vadoola/dxf2elmt)

## How to Use

### CLI Version

dxf2elmt requires only one input from the user, the input file.

For example:

```bash
./dxf2elmt my_file.dxf
```

The .elmt file will be output into the same directory as the executable. It will retain the name of the .dxf file.

If you wish to forgo creating an .elmt file, you can use the "-v" argument for verbose output. This will output the contents of the .elmt file to stdout without actually creating the file. For example:

```bash
./dxf2elmt my_file.dxf -v
```

Additional options:
- `-s, --spline-step <NUMBER>`: Determine the number of lines you want each spline to have (more lines = greater resolution). Default: 20
- `-i, --info`: Display conversion statistics
- `-d, --dtext`: Convert text entities into dynamic text instead of the default text box

### Desktop Version

The desktop version provides a graphical user interface where you can:

1. Select a DXF file using the file picker
2. Preview entity statistics before conversion
3. Configure conversion options:
   - **Spline step**: Number of points to approximate splines (1-200, default: 20)
   - **Pixels/mm ratio**: Configure the pixel-to-millimeter conversion ratio (default: 2 px/mm)
   - **Verbose mode**: Print XML output instead of writing to file
   - **Info mode**: Display conversion statistics
4. Convert the file and open the output directory

The desktop application automatically handles unit conversion from DXF units to ELMT pixels based on the configured ratio.

## Supported Entities

* Lines
* Circles
* Arcs
* Texts
* Ellipses
* Polylines
* LwPolylines
* Solids
* Splines
* Blocks (there are still some known issues for deeply nested blocks)
* MText (partial support)
* Leader

## Unit Conversion

The program supports automatic unit conversion from various DXF units (millimeters, centimeters, meters, inches, feet, etc.) to ELMT pixels. The conversion ratio is configurable in the desktop version (default: 2 pixels per millimeter).

## To Do

* Support for the following:
    * Remaining 2D entities
    * Styling (such as Dimension Styles)
* Better error messages
* Logging improvements
* Enhanced block nesting support

## Compiling

Compiled using:
- **Rust**: MSRV 1.79.0
- **Dioxus**: 0.7 (for desktop version)

To build the project:

```bash
# Build CLI version
cargo build --release --bin dxf2elmt

# Build desktop version
cargo build --release --bin dxf2elmt-desktop
```

## Credits

* [tonilupi] ([www.labeltec.com](https://github.com/labeltec)) - Author and maintainer of this desktop version
* [Antonioaja](https://github.com/antonioaja) for creating the initial versions of [dxf2elmt](https://github.com/antonioaja/dxf2elmt). Thank you for all your work.
* [Vadoola](https://github.com/Vadoola) for maintaining and improving the original project
* [QElectroTech](https://qelectrotech.org/)
* [dxf-rs](https://github.com/IxMilia/dxf-rs)
* [simple-xml-builder](https://github.com/Accelbread/simple-xml-builder)
* [bspline](https://github.com/Twinklebear/bspline)
* [tempfile](https://github.com/Stebalien/tempfile)
* [Dioxus](https://dioxuslabs.com/) for the desktop framework
