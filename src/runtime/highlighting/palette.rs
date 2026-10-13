
use reedline::Color;

use crate::{ language::analysis::highlight::HighlightKind, runtime::color::TtyColorMode };


pub const COLOR_COUNT: usize = HighlightKind::Error as usize + 1;


// Foreground-only defaults drawn from the logo's violet, cyan, blue, and warm highlights.
pub const SYNTAX_COLORS: &[(HighlightKind, &str, &str)] = &[
        (HighlightKind::Comment, "$syntax_color_comment", "\x1b[38;2;125;137;156m"),
        (HighlightKind::Keyword, "$syntax_color_keyword", "\x1b[38;2;177;120;255m"),
        (HighlightKind::Variable, "$syntax_color_variable", "\x1b[38;2;255;205;110m"),
        (HighlightKind::Type, "$syntax_color_type", "\x1b[38;2;88;235;200m"),
        (HighlightKind::Function, "$syntax_color_function", "\x1b[38;2;110;175;255m"),
        (HighlightKind::Method, "$syntax_color_method", "\x1b[38;2;118;206;255m"),
        (HighlightKind::Builtin, "$syntax_color_builtin", "\x1b[38;2;156;157;255m"),
        (HighlightKind::Alias, "$syntax_color_alias", "\x1b[38;2;202;160;255m"),
        (HighlightKind::Command, "$syntax_color_command", "\x1b[38;2;49;216;224m"),
        (HighlightKind::Argument, "$syntax_color_argument", "\x1b[38;2;215;222;233m"),
        (HighlightKind::String, "$syntax_color_string", "\x1b[38;2;108;226;191m"),
        (HighlightKind::Number, "$syntax_color_number", "\x1b[38;2;255;205;110m"),
        (HighlightKind::Literal, "$syntax_color_literal", "\x1b[38;2;205;153;255m"),
        (HighlightKind::Operator, "$syntax_color_operator", "\x1b[38;2;86;205;225m"),
        (HighlightKind::Delimiter, "$syntax_color_delimiter", "\x1b[38;2;174;184;200m"),
        (HighlightKind::Field, "$syntax_color_field", "\x1b[38;2;151;199;255m"),
        (HighlightKind::Module, "$syntax_color_module", "\x1b[38;2;131;160;255m"),
        (HighlightKind::Format, "$syntax_color_format", "\x1b[38;2;49;216;224m"),
        (HighlightKind::Escape, "$syntax_color_escape", "\x1b[38;2;255;169;130m"),
        (HighlightKind::Error, "$syntax_color_error", "\x1b[38;2;255;112;142m"),
    ];


#[derive(Clone)]
pub struct SyntaxSettings
{
    pub enabled: bool,
    pub colors: [Option<Color>; COLOR_COUNT],
    pub mode: TtyColorMode,
}

impl SyntaxSettings
{
    pub fn new(mode: TtyColorMode) -> Self
    {
        let mut settings = Self { enabled: true, colors: [None; COLOR_COUNT], mode };
        for &(kind, _, color) in SYNTAX_COLORS
        { settings.colors[kind as usize] = foreground(color, mode).unwrap(); }
        settings
    }

    pub fn set_color(&mut self, kind: HighlightKind, value: &str)
    {
        if let Some(color) = foreground(value, self.mode)
        { self.colors[kind as usize] = color; }
    }
}


/**
 * Configuration is never copied into terminal output. Accept one foreground SGR only;
 * empty text and the default foreground intentionally disable a category's coloring.
 */
fn foreground(value: &str, mode: TtyColorMode) -> Option<Option<Color>>
{
    if value.is_empty() { return Some(None); }
    let sequence = value.strip_prefix("\x1b[")?.strip_suffix('m')?;
    let numbers: Vec<u8> = sequence.split(';').map(str::parse).collect::<Result<_, _>>().ok()?;
    let color = match numbers.as_slice()
        {
            [0] | [39] => return Some(None),
            [code @ 30..=37] => basic_color(code - 30),
            [code @ 90..=97] => basic_color(code - 90 + 8),
            [38, 5, index] => Color::Fixed(*index),
            [38, 2, red, green, blue] => Color::Rgb(*red, *green, *blue),
            _ => return None,
        };
    Some(match mode
        {
            TtyColorMode::TtyMonochrome => None,
            TtyColorMode::TtyBasic => Some(nearest(color, 16)),
            TtyColorMode::Tty256 => Some(match color
                {
                    Color::Rgb(..) => nearest(color, 256),
                    color => color,
                }),
            TtyColorMode::TtyTrueColor => Some(color),
        })
}


fn basic_color(index: u8) -> Color
{
    match index
    {
        0 => Color::Black, 1 => Color::Red, 2 => Color::Green, 3 => Color::Yellow,
        4 => Color::Blue, 5 => Color::Purple, 6 => Color::Cyan, 7 => Color::White,
        8 => Color::DarkGray, 9 => Color::LightRed, 10 => Color::LightGreen,
        11 => Color::LightYellow, 12 => Color::LightBlue, 13 => Color::LightPurple,
        14 => Color::LightCyan, _ => Color::LightGray,
    }
}


fn rgb(index: u8) -> (u8, u8, u8)
{
    const BASIC: [(u8, u8, u8); 16] = [
            (0, 0, 0), (128, 0, 0), (0, 128, 0), (128, 128, 0),
            (0, 0, 128), (128, 0, 128), (0, 128, 128), (192, 192, 192),
            (128, 128, 128), (255, 0, 0), (0, 255, 0), (255, 255, 0),
            (0, 0, 255), (255, 0, 255), (0, 255, 255), (255, 255, 255),
        ];
    if index < 16 { return BASIC[index as usize]; }
    if index >= 232 { let gray = 8 + (index - 232) * 10; return (gray, gray, gray); }
    let index = index - 16;
    let levels = [0, 95, 135, 175, 215, 255];
    (levels[(index / 36) as usize], levels[(index / 6 % 6) as usize], levels[(index % 6) as usize])
}


fn nearest(color: Color, count: u16) -> Color
{
    let wanted = match color
        {
            Color::Rgb(red, green, blue) => (red, green, blue),
            Color::Fixed(index) => rgb(index),
            color => return color,
        };
    let chromatic = wanted.0.max(wanted.1).max(wanted.2)
        - wanted.0.min(wanted.1).min(wanted.2) > 32;
    let index = (0..count).filter(|index|
        count != 16 || !chromatic || !matches!(*index, 0 | 7 | 8 | 15)).min_by_key(|index|
        {
            let candidate = rgb(*index as u8);
            let delta = |left: u8, right: u8| (left as i32 - right as i32).pow(2);
            delta(wanted.0, candidate.0) + delta(wanted.1, candidate.1)
                + delta(wanted.2, candidate.2)
        }).unwrap() as u8;
    if count == 16 { basic_color(index) } else { Color::Fixed(index) }
}

pub(crate) fn adapt_color(color: Color, mode: TtyColorMode) -> Option<Color>
{
    match mode
    {
        TtyColorMode::TtyMonochrome => None,
        TtyColorMode::TtyBasic => Some(nearest(color, 16)),
        TtyColorMode::Tty256 => Some(nearest(color, 256)),
        TtyColorMode::TtyTrueColor => Some(color),
    }
}
