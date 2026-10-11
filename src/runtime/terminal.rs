
use std::{ io::{ Error, ErrorKind, IsTerminal, Write, stdin, stdout },
           mem::MaybeUninit,
           str::from_utf8,
           time::{ Duration, Instant } };

use crossterm::{ event::{ Event, KeyCode, KeyEvent, KeyModifiers }, terminal::window_size };

use libc::{ FIONREAD, ICANON, POLLIN, STDIN_FILENO, TCSANOW, VEOL, VEOL2, VEOF,
            VERASE, VKILL, _POSIX_VDISABLE, cfmakeraw, getpgrp, ioctl,
            poll, pollfd, read, tcgetattr, tcgetpgrp, tcsetattr, termios };

#[derive(Default)]
pub struct TerminalCapabilities
{
    pub sixel: bool,
    pub cell_size: Option<(u16, u16)>,
    pub screen_size: Option<(u16, u16)>,
    pub input: Vec<Event>,
}

struct TerminalMode(termios);

impl Drop for TerminalMode
{
    fn drop(&mut self)
    {
        // Never flush input: queued keystrokes belong to the line editor.
        unsafe { tcsetattr(STDIN_FILENO, TCSANOW, &self.0); }
    }
}

fn read_byte(deadline: Instant) -> Option<u8>
{
    loop
    {
        let remaining = deadline.checked_duration_since(Instant::now())?;
        let mut descriptor = pollfd { fd: STDIN_FILENO, events: POLLIN, revents: 0 };
        let timeout = remaining.as_millis().clamp(1, i32::MAX as u128) as i32;
        let ready = unsafe { poll(&mut descriptor, 1, timeout) };
        if ready < 0
        {
            if Error::last_os_error().kind() == ErrorKind::Interrupted
            { continue; }
            return None;
        }
        if ready == 0 || descriptor.revents & POLLIN == 0 { return None; }
        let mut byte = 0;
        if unsafe { read(STDIN_FILENO, (&mut byte as *mut u8).cast(), 1) } == 1
        { return Some(byte); }
        return None;
    }
}

fn character_input(first: u8, deadline: Instant) -> Vec<Event>
{
    let (code, modifiers) = match first
        {
            b'\r' | b'\n' => (KeyCode::Enter, KeyModifiers::NONE),
            b'\t' => (KeyCode::Tab, KeyModifiers::NONE),
            8 | 127 => (KeyCode::Backspace, KeyModifiers::NONE),
            1..=26 => (KeyCode::Char((b'a' + first - 1) as char), KeyModifiers::CONTROL),
            27 => (KeyCode::Esc, KeyModifiers::NONE),
            _ =>
                {
                    let length = match first
                        {
                            0xc2..=0xdf => 2,
                            0xe0..=0xef => 3,
                            0xf0..=0xf4 => 4,
                            _ => 1,
                        };
                    let mut bytes = vec![first];
                    for _ in 1..length
                    {
                        let Some(byte) = read_byte(deadline) else { break; };
                        bytes.push(byte);
                    }
                    return vec![Event::Paste(String::from_utf8_lossy(&bytes).into_owned())];
                }
        };
    vec![Event::Key(KeyEvent::new(code, modifiers))]
}

fn escape_input(bytes: &[u8]) -> Vec<Event>
{
    let code = match bytes
        {
            b"\x1b[A" | b"\x1bOA" => Some(KeyCode::Up),
            b"\x1b[B" | b"\x1bOB" => Some(KeyCode::Down),
            b"\x1b[C" | b"\x1bOC" => Some(KeyCode::Right),
            b"\x1b[D" | b"\x1bOD" => Some(KeyCode::Left),
            b"\x1b[H" | b"\x1bOH" | b"\x1b[1~" | b"\x1b[7~" => Some(KeyCode::Home),
            b"\x1b[F" | b"\x1bOF" | b"\x1b[4~" | b"\x1b[8~" => Some(KeyCode::End),
            b"\x1b[3~" => Some(KeyCode::Delete),
            b"\x1b" => Some(KeyCode::Esc),
            _ => None,
        };
    if let Some(code) = code
    { return vec![Event::Key(KeyEvent::new(code, KeyModifiers::NONE))]; }
    let alt_text = bytes.strip_prefix(b"\x1b").and_then(|text| from_utf8(text).ok());
    if    let Some(text) = alt_text
       && text.chars().count() == 1
    {
        return vec![Event::Key(KeyEvent::new(
            KeyCode::Char(text.chars().next().unwrap()), KeyModifiers::ALT))];
    }
    if bytes.starts_with(b"\x1b[") && bytes.len() > 3
    {
        let parameters = from_utf8(&bytes[2..bytes.len() - 1]).ok()
            .and_then(|text| text.split(';').map(str::parse::<u16>)
                .collect::<Result<Vec<_>, _>>().ok());
        if let Some(parameters) = parameters
        {
            let first = parameters.first().copied().unwrap_or(1);
            let mask = parameters.get(1).copied().unwrap_or(1).saturating_sub(1);
            let mut modifiers = KeyModifiers::NONE;
            if mask & 1 != 0 { modifiers |= KeyModifiers::SHIFT; }
            if mask & 2 != 0 { modifiers |= KeyModifiers::ALT; }
            if mask & 4 != 0 { modifiers |= KeyModifiers::CONTROL; }
            let code = match bytes.last()
                {
                    Some(b'A') => Some(KeyCode::Up),
                    Some(b'B') => Some(KeyCode::Down),
                    Some(b'C') => Some(KeyCode::Right),
                    Some(b'D') => Some(KeyCode::Left),
                    Some(b'H') => Some(KeyCode::Home),
                    Some(b'F') => Some(KeyCode::End),
                    Some(b'~') if first == 3 => Some(KeyCode::Delete),
                    Some(b'u') if first == 13 => Some(KeyCode::Enter),
                    Some(b'u') => char::from_u32(first as u32).map(KeyCode::Char),
                    _ => None,
                };
            if let Some(code) = code
            { return vec![Event::Key(KeyEvent::new(code, modifiers))]; }
        }
    }
    Vec::new()
}

/// Query Primary DA and cell dimensions only on an idle foreground terminal.
/// Attribute 4, after the terminal identifier, advertises sixel. Both queries
/// share a 150 ms deadline; kernel window dimensions provide a cell-size fallback.
/// https://invisible-island.net/xterm/ctlseqs/ctlseqs.html
pub fn detect_capabilities() -> TerminalCapabilities
{
    let mut result = TerminalCapabilities::default();
    if !stdin().is_terminal() || !stdout().is_terminal() { return result; }
    if unsafe { tcgetpgrp(STDIN_FILENO) } != unsafe { getpgrp() } { return result; }
    if    let Ok(size) = window_size()
       && size.columns > 0 && size.rows > 0
    {
        result.screen_size = Some((size.columns, size.rows));
        result.cell_size = valid_cell_size(size.width / size.columns, size.height / size.rows);
    }

    let mut pending = 0;
    if unsafe { ioctl(STDIN_FILENO, FIONREAD as _, &mut pending) } != 0 || pending != 0
    { return result; }
    let mut original = MaybeUninit::<termios>::uninit();
    if unsafe { tcgetattr(STDIN_FILENO, original.as_mut_ptr()) } != 0 { return result; }
    let original = unsafe { original.assume_init() };
    let mut query_mode = original;
    unsafe { cfmakeraw(&mut query_mode); }
    // Keep incomplete replies queued until their final byte ('c', then 't').
    // After a timeout Reedline can still parse a late reply from its beginning.
    // Disable canonical editing so user keystrokes remain unchanged.
    query_mode.c_lflag |= ICANON;
    for control in [VEOF, VEOL2, VERASE, VKILL]
    { query_mode.c_cc[control] = _POSIX_VDISABLE; }
    query_mode.c_cc[VEOL] = b'c';
    if unsafe { tcsetattr(STDIN_FILENO, TCSANOW, &query_mode) } != 0 { return result; }
    let _mode = TerminalMode(original);
    let mut output = stdout().lock();
    if output.write_all(b"\x1b[c").and_then(|_| output.flush()).is_err() { return result; }
    drop(output);

    let deadline = Instant::now() + Duration::from_millis(150);
    let Some(response) = read_reply(deadline, &mut result.input) else { return result; };
    if response.starts_with(b"\x1b[?") && response.last() == Some(&b'c')
    {
        let attributes = from_utf8(&response[3..response.len() - 1]).ok()
            .and_then(|text| text.split(';').map(str::parse::<u16>)
                .collect::<Result<Vec<_>, _>>().ok());
        result.sixel = attributes.is_some_and(|values|
            values.iter().skip(1).any(|value| *value == 4));
    }
    if !result.sixel || result.cell_size.is_some() { return result; }

    query_mode.c_cc[VEOL] = b't';
    if unsafe { tcsetattr(STDIN_FILENO, TCSANOW, &query_mode) } != 0 { return result; }
    let mut output = stdout().lock();
    if output.write_all(b"\x1b[16t").and_then(|_| output.flush()).is_err() { return result; }
    drop(output);
    if    let Some(response) = read_reply(deadline, &mut result.input)
       && response.starts_with(b"\x1b[6;")
       && response.last() == Some(&b't')
    {
        let dimensions = from_utf8(&response[4..response.len() - 1]).ok()
            .and_then(|text| text.split(';').map(str::parse::<u16>)
                .collect::<Result<Vec<_>, _>>().ok());
        if let Some(values) = dimensions.filter(|values| values.len() == 2)
        { result.cell_size = valid_cell_size(values[1], values[0]); }
    }
    result
}

fn valid_cell_size(width: u16, height: u16) -> Option<(u16, u16)>
{
    if (1..=128).contains(&width) && (1..=128).contains(&height)
    { Some((width, height)) }
    else { None }
}

fn read_reply(deadline: Instant, input: &mut Vec<Event>) -> Option<Vec<u8>>
{
    let first = read_byte(deadline)?;
    if first != 27
    {
        input.extend(character_input(first, deadline));
        return None;
    }
    let mut response = vec![first];
    while response.len() < 4096
    {
        let Some(byte) = read_byte(deadline) else { break; };
        response.push(byte);
        if response.starts_with(b"\x1b[?")
        {
            if byte == b'c' { break; }
        }
        else if response.starts_with(b"\x1b[6;")
        {
            if byte == b't' { break; }
        }
        else if response.len() >= 3 && (0x40..=0x7e).contains(&byte) { break; }
        if response.len() == 2 && byte != b'[' && byte != b'O'
        {
            let length = match byte
                {
                    0xc2..=0xdf => 2,
                    0xe0..=0xef => 3,
                    0xf0..=0xf4 => 4,
                    _ => 1,
                };
            for _ in 1..length
            {
                let Some(byte) = read_byte(deadline) else { break; };
                response.push(byte);
            }
            break;
        }
    }
    if response.starts_with(b"\x1b[?") || response.starts_with(b"\x1b[6;")
    { Some(response) }
    else
    {
        input.extend(escape_input(&response));
        None
    }
}
