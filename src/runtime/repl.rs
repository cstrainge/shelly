
use std::{ borrow::Cow,
           collections::BTreeMap,
           process::ExitCode,
           path::{ Path, PathBuf },
           fs::read_dir,
           env::{ current_dir, home_dir, split_paths } };

use reedline::{ Color,
                ColumnarMenu,
                Completer,
                CompletionResult,
                EditMode,
                Emacs,
                InputMode,
                Keybindings,
                KeyModifiers,
                KeyCode,
                MenuBuilder,
                Prompt,
                PromptEditMode,
                PromptHistorySearch,
                Reedline,
                ReedlineMenu,
                ReedlineRawEvent,
                Signal,
                Span,
                Suggestion,
                ReedlineEvent,
                EditCommand };

use rustix::process::geteuid;

use crate::{ language::{ interpreter::{ Interpreter, Interactive, RcFile, Startup },
                         text::buffer::SimpleBuffer },
             runtime::{ color::TtyColorMode, result::RuntimeResult },
             location_here };

struct ShellyPrompt
{
    prompt_text: String
}


impl Prompt for ShellyPrompt
{
    fn render_prompt_left(&self) -> Cow<'_, str>
    {
        Cow::Owned(self.prompt_text.clone())
    }

    fn render_prompt_right(&self) -> Cow<'_, str>
    {
        Cow::Borrowed("")
    }

    fn render_prompt_indicator(&self, _mode: PromptEditMode) -> Cow<'_, str>
    {
        if geteuid().is_root()
        {
            return Cow::Borrowed("# ");
        }

        Cow::Borrowed("$ ")
    }

    fn render_prompt_multiline_indicator(&self) -> Cow<'_, str>
    {
        Cow::Borrowed("> ")
    }

    fn render_prompt_history_search_indicator(&self, _search: PromptHistorySearch) -> Cow<'_, str>
    {
        Cow::Borrowed("search> ")
    }

    fn get_prompt_color(&self) -> Color
    {
        Color::LightGray
    }

    fn get_indicator_color(&self) -> Color
    {
        Color::LightGray
    }

    fn get_prompt_multiline_color(&self) -> Color
    {
        Color::DarkGray
    }
}


#[derive(Clone)]
struct ShellyCompleter
{
    search_path: Vec<PathBuf>,
    home: Option<PathBuf>,
    variables: Vec<String>
}


// Return only the variable reference at the cursor, preserving surrounding text.
fn completion_variable(line: &str, pos: usize) -> Option<(Span, &str, bool)>
{
    let before = line.get(..pos)?;
    let mut start = None;
    let mut quote = None;
    let mut escaped = false;
    let mut comment = false;

    for (index, character) in before.char_indices()
    {
        if comment
        {
            if character == '\n' { comment = false; }
            continue;
        }
        if escaped
        {
            escaped = false;
            continue;
        }
        if character == '\\' && quote.is_some()
        {
            escaped = true;
            continue;
        }
        if let Some(delimiter) = quote
        {
            if character == delimiter
            {
                quote = None;
                start = None;
                continue;
            }
            if delimiter == '\'' { continue; }
        }
        else if matches!(character, '\'' | '"')
        {
            quote = Some(character);
            start = None;
            continue;
        }
        else if character == '#'
        {
            comment = true;
            start = None;
            continue;
        }

        if character == '$' { start = Some(index); }
    }

    if comment || escaped || quote == Some('\'') { return None; }
    let start = start?;
    let reference = &before[start + 1..];
    let braced = reference.starts_with('{');
    let prefix = if braced { &reference[1..] } else { reference };
    let name_character = |c: char| c.is_alphanumeric() || c == '_';
    if !prefix.chars().all(name_character) { return None; }

    let mut end = pos;
    for character in line[pos..].chars().take_while(|c| name_character(*c))
    {
        end += character.len_utf8();
    }
    if braced && line[end..].starts_with('}') { end += 1; }

    Some((Span::new(start, end), prefix, braced))
}


// Find the word at the cursor without treating spaces inside quotes as separators.
fn completion_word(line: &str, pos: usize) -> Option<(Span, String, bool)>
{
    let before = line.get(..pos)?;
    let mut start = 0;
    let mut word = String::new();
    let mut quote = None;
    let mut escaped = false;
    let mut command = true;
    let mut comment = false;

    for (index, character) in before.char_indices()
    {
        if comment && character != '\n' { continue; }
        if escaped
        {
            word.push(match character { 'n' => '\n', 'r' => '\r', 't' => '\t', _ => character });
            escaped = false;
        }
        else if let Some(delimiter) = quote
        {
            if character == '\\' { escaped = true; }
            else if character == delimiter { quote = None; }
            else { word.push(character); }
        }
        else if matches!(character, '\'' | '"')
        {
            quote = Some(character);
        }
        else if character == '#'
        {
            comment = true;
        }
        else if matches!(character, ';' | '|' | '\n' | '{' | '}')
        {
            start = index + character.len_utf8();
            word.clear();
            command = true;
            comment = false;
        }
        else if character.is_whitespace()
        {
            if start < index { command = false; }
            start = index + character.len_utf8();
            word.clear();
        }
        else
        {
            word.push(character);
        }
    }

    if comment || escaped { return None; }

    // Replace the rest of the word too when completing in the middle of a line.
    let mut end = pos;
    for character in line[pos..].chars()
    {
        if escaped { escaped = false; }
        else if let Some(delimiter) = quote
        {
            if character == '\\' { escaped = true; }
            else if character == delimiter { quote = None; }
        }
        else if matches!(character, '\'' | '"') { quote = Some(character); }
        else if character.is_whitespace() || matches!(character, ';' | '|' | '{' | '}' | '#')
        {
            break;
        }
        end += character.len_utf8();
    }

    Some((Span::new(start, end), word, command))
}


fn completion_text(path: &str, directory: bool) -> String
{
    // Single quotes protect literal dollars and glob characters in filenames.
    if !path.is_empty() && path.chars().all(|c| c.is_alphanumeric() || "_./-".contains(c))
    {
        return path.to_string();
    }

    let escaped = path.replace('\\', "\\\\").replace('\'', "\\'")
        .replace('\n', "\\n").replace('\r', "\\r").replace('\t', "\\t");
    // Leave directory quotes open so the next component can be typed inside them.
    format!("'{}{}", escaped, if directory { "" } else { "'" })
}


fn is_executable(path: &Path) -> bool
{
    let Ok(metadata) = path.metadata() else { return false; };
    if !metadata.is_file() { return false; }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}


impl Completer for ShellyCompleter
{
    fn complete(&mut self, line: &str, pos: usize) -> CompletionResult
    {
        if let Some((span, prefix, braced)) = completion_variable(line, pos)
        {
            return CompletionResult::fresh(self.variables.iter().filter_map(|variable|
                {
                    let name = variable.strip_prefix('$')?;
                    if !name.starts_with(prefix) { return None; }
                    if !braced && !name.chars().all(|c| c.is_alphanumeric() || c == '_')
                    {
                        return None;
                    }
                    Some(Suggestion
                        {
                            value: if braced { format!("${{{}}}", name) } else { variable.clone() },
                            span,
                            append_whitespace: false,
                            ..Suggestion::default()
                        })
                }).collect::<Vec<_>>());
        }

        let Some((span, word, command)) = completion_word(line, pos) else
        {
            return CompletionResult::fresh(Vec::new());
        };

        let mut matches = BTreeMap::new();

        if command && !word.contains('/')
        {
            for directory in &self.search_path
            {
                let directory = if directory.as_os_str().is_empty()
                {
                    Path::new(".")
                }
                else
                {
                    directory
                };
                let Ok(entries) = read_dir(directory) else { continue; };
                for entry in entries.flatten()
                {
                    let Ok(name) = entry.file_name().into_string() else { continue; };
                    if name.starts_with(&word) && is_executable(&entry.path())
                    {
                        matches.insert(name, false);
                    }
                }
            }
        }

        // Expand home prefixes while resolving completion candidates.
        let path = if    word == "~"
                      || word.starts_with("~/")
                      || word == "$HOME"
                      || word.starts_with("$HOME/")
            {
                let Some(home) = &self.home else { return CompletionResult::fresh(Vec::new()); };
                let suffix = word
                    .strip_prefix('~')
                    .unwrap_or_else(|| word.strip_prefix("$HOME").unwrap());
                format!("{}/{}", home.display(), suffix.trim_start_matches('/'))
            }
            else
            {
                word.clone()
            };

        let (prefix, filename) = path.rfind('/').map_or(("", path.as_str()),
            |index| (&path[..=index], &path[index + 1..]));
        let directory = if prefix.is_empty() { Path::new(".") } else { Path::new(prefix) };

        if let Ok(entries) = read_dir(directory)
        {
            for entry in entries.flatten()
            {
                let Ok(name) = entry.file_name().into_string() else { continue; };
                if    !name.starts_with(filename)
                   || (name.starts_with('.') && !filename.starts_with('.'))
                {
                    continue;
                }

                let directory = entry.path().is_dir();
                let local_prefix = if command && prefix.is_empty() { "./" } else { prefix };
                let value = format!("{}{}{}", local_prefix, name, if directory { "/" } else { "" });
                matches.insert(value, directory);
            }
        }

        CompletionResult::fresh(matches.into_iter().map(|(path, directory)| Suggestion
            {
                value: completion_text(&path, directory),
                display_override: Some(path),
                span,
                append_whitespace: !directory,
                ..Suggestion::default()
            }).collect::<Vec<_>>())
    }
}


struct FirstTabCompleter(ShellyCompleter);

impl Completer for FirstTabCompleter
{
    fn complete(&mut self, line: &str, pos: usize) -> CompletionResult
    {
        let result = self.0.complete(line, pos);
        let suggestions = result.suggestions();
        if suggestions.len() == 1 { return result; }

        // A single suggestion is accepted silently by Reedline's quick completion.
        // With no extension to offer, accept a no-op rather than opening a menu.
        let mut suggestion = Suggestion
            {
                span: Span::new(pos, pos),
                ..Suggestion::default()
            };

        if let Some(first) = suggestions.first()
        {
            let mut prefix = first.display_value().to_string();
            for other in &suggestions[1..]
            {
                let shared_bytes = prefix.chars().zip(other.display_value().chars())
                    .take_while(|(left, right)| left == right)
                    .map(|(character, _)| character.len_utf8()).sum();
                prefix.truncate(shared_bytes);
            }

            if !prefix.is_empty()
            {
                suggestion.span = first.span;
                if let Some((_, _, braced)) = completion_variable(line, pos)
                {
                    // Keep an existing closing brace beyond the cursor on partial completion.
                    if braced && line[..first.span.end].ends_with('}') && !prefix.ends_with('}')
                    {
                        suggestion.span.end -= 1;
                    }
                    suggestion.value = prefix;
                }
                else
                {
                    // A partial path may still need more characters inside its quotes.
                    suggestion.value = completion_text(&prefix, true);
                }
            }
        }

        CompletionResult::fresh(vec![suggestion])
    }
}


struct ShellyEditMode
{
    emacs: Emacs,
    previous_tab: bool
}

impl EditMode for ShellyEditMode
{
    fn parse_event(&mut self, event: ReedlineRawEvent) -> ReedlineEvent
    {
        let event = self.emacs.parse_event(event);
        let tab = matches!(&event, ReedlineEvent::UntilFound(events)
            if matches!(events.first(), Some(ReedlineEvent::Menu(name))
                if name == "completion_menu"));
        let first_tab = tab && !self.previous_tab;
        self.previous_tab = tab;

        if first_tab
        {
            ReedlineEvent::UntilFound(vec![
                    ReedlineEvent::Menu("first_tab".to_string()),
                    ReedlineEvent::MenuNext
                ])
        }
        else { event }
    }

    fn edit_mode(&self) -> PromptEditMode
    {
        self.emacs.edit_mode()
    }
}


fn default_prompt(interpreter: &Interpreter) -> String
{
    let cwd = interpreter.evaluate_variable("$pwd").unwrap_or_else(|_|
        current_dir().unwrap_or_else(|_| ".".into()).display().to_string());

    let formatted = format!("\n{} [{}]\n",
                            Color::Yellow.bold().paint("<shelly>"),
                            Color::Cyan.paint(cwd));

    formatted
}


pub struct Repl
{
    tab_width: usize,
    interpreter: Interpreter
}


impl Repl
{
    pub fn new(color_mode: TtyColorMode,
               tab_width: usize,
               startup: Startup,
               suppress_banner: bool,
               rc_file: RcFile,
               script_args: Vec<String>) -> Self
    {
        let interactive_mode = if suppress_banner
            {
                Interactive::YesWithoutBanner
            }
            else
            {
                Interactive::Yes
            };

        let interpreter = Interpreter::new(startup,
                                           interactive_mode,
                                           color_mode,
                                           tab_width,
                                           rc_file,
                                           script_args);

        Self
            {
                tab_width,
                interpreter
            }
    }

    pub fn run(&mut self) -> RuntimeResult<ExitCode>
    {
        let mut prompt = ShellyPrompt { prompt_text: String::new() };
        let mut keybindings = Keybindings::empty();

        Self::apply_keybindings(&mut keybindings);

        let mut editor = Reedline::create()
            .use_kitty_keyboard_enhancement(true)
            .with_quick_completions(true)
            .with_edit_mode(Box::new(ShellyEditMode
                {
                    emacs: Emacs::new(keybindings),
                    previous_tab: false
                }));

        while !self.interpreter.halted
        {
            let prompt_text = if self.interpreter.has_command("prompt")
                {
                    let (result, bytes) = self.interpreter.capture_stdout(|interpreter|
                        {
                            interpreter.execute_command(location_here!(),
                                                        "prompt",
                                                        vec![])
                        });

                    if result.is_err()
                    {
                        default_prompt(&self.interpreter)
                    }
                    else
                    {
                        String::from_utf8_lossy(&bytes).to_string()
                    }
                }
                else
                {
                    default_prompt(&self.interpreter)
                };

            prompt.prompt_text = prompt_text;

            // Refresh after each command so changes to PATH, HOME, and cwd are respected.
            let search_path = self.interpreter.evaluate_path_variable("$PATH").unwrap_or_default();
            let home = self.interpreter.evaluate_path_variable("$HOME").ok()
                .filter(|home| !home.is_empty()).map(PathBuf::from).or_else(home_dir);
            let completer = ShellyCompleter
                {
                    search_path: split_paths(&search_path).collect(),
                    home,
                    variables: self.interpreter.variable_names()
                };
            editor = editor.clear_menus()
                .with_completer(Box::new(completer.clone()))
                .with_menu(ReedlineMenu::EngineCompleter(Box::new(
                    ColumnarMenu::default().with_name("completion_menu")
                        .with_input_mode(InputMode::FullBuffer))))
                .with_menu(ReedlineMenu::WithCompleter
                    {
                        menu: Box::new(ColumnarMenu::default().with_name("first_tab")
                            .with_input_mode(InputMode::FullBuffer)),
                        completer: Box::new(FirstTabCompleter(completer))
                    });

            match editor.read_line(&prompt)
            {
                Ok(Signal::Success(text)) =>
                    {
                        let mut buffer = SimpleBuffer::new("<repl>", &text, Some(self.tab_width));
                        let result = self.interpreter.execute_from_buffer(&mut buffer);

                        if let Err(error) = result
                        {
                            println!("Error processing text: {}", error);
                            continue;
                        }

                        if self.interpreter.halted
                        {
                            break;
                        }
                    },

                Ok(Signal::CtrlC) =>
                    {
                        println!("Ctrl+C pressed.");
                    }

                Ok(Signal::CtrlD) =>
                    {
                        println!("Ctrl+D pressed. Exiting.");
                        break;
                    }

                Ok(_) =>
                    {
                        println!("Unhandled signal received.");
                    }

                Err(error) =>
                    {
                        println!("Error reading line: {:?}.", error);
                    }
            }
        }

        Ok(ExitCode::from(self.interpreter.exit_code))
    }

    fn apply_keybindings(keybindings: &mut Keybindings)
    {
        fn simple(command: EditCommand) -> ReedlineEvent
        {
            ReedlineEvent::Edit(vec![command])
        }

        fn binding(keybindings: &mut Keybindings, key_code: KeyCode, event: ReedlineEvent)
        {
            keybindings.add_binding(KeyModifiers::NONE, key_code, event);
        }

        fn ctrl_binding(keybindings: &mut Keybindings, key_code: KeyCode, event: ReedlineEvent)
        {
            keybindings.add_binding(KeyModifiers::CONTROL, key_code, event);
        }

        fn shift_binding(keybindings: &mut Keybindings, key_code: KeyCode, event: ReedlineEvent)
        {
            keybindings.add_binding(KeyModifiers::SHIFT, key_code, event);
        }

        binding(keybindings, KeyCode::Enter, ReedlineEvent::Enter);
        binding(keybindings, KeyCode::Tab, ReedlineEvent::UntilFound(vec![
                ReedlineEvent::Menu("completion_menu".to_string()),
                ReedlineEvent::MenuNext
            ]));
        shift_binding(keybindings, KeyCode::BackTab, ReedlineEvent::MenuPrevious);

        binding(keybindings, KeyCode::Left, ReedlineEvent::UntilFound(vec![
                ReedlineEvent::MenuLeft, ReedlineEvent::Left
            ]));
        binding(keybindings, KeyCode::Right, ReedlineEvent::UntilFound(vec![
                ReedlineEvent::MenuRight, ReedlineEvent::Right
            ]));
        binding(keybindings, KeyCode::Up, ReedlineEvent::UntilFound(vec![
                ReedlineEvent::MenuUp, ReedlineEvent::Up
            ]));
        binding(keybindings, KeyCode::Down, ReedlineEvent::UntilFound(vec![
                ReedlineEvent::MenuDown, ReedlineEvent::Down
            ]));

        binding(keybindings, KeyCode::Backspace, simple(EditCommand::Backspace));
        binding(keybindings, KeyCode::Delete, simple(EditCommand::Delete));
        binding(keybindings, KeyCode::Home, simple(EditCommand::MoveToLineStart { select: false }));
        binding(keybindings, KeyCode::End, simple(EditCommand::MoveToLineEnd { select: false }));

        ctrl_binding(keybindings, KeyCode::Char('c'), ReedlineEvent::CtrlC);
        ctrl_binding(keybindings, KeyCode::Char('d'), ReedlineEvent::CtrlD);
        ctrl_binding(keybindings, KeyCode::Char('z'), simple(EditCommand::Undo));
        ctrl_binding(keybindings, KeyCode::Char('y'), simple(EditCommand::Redo));
        ctrl_binding(
            keybindings,
            KeyCode::Left,
            simple(EditCommand::MoveWordLeft { select: false }),
        );
        ctrl_binding(
            keybindings,
            KeyCode::Right,
            simple(EditCommand::MoveWordRight { select: false }),
        );
        ctrl_binding(keybindings, KeyCode::Backspace, simple(EditCommand::BackspaceWord));
        ctrl_binding(keybindings, KeyCode::Delete, simple(EditCommand::DeleteWord));

        ctrl_binding(keybindings, KeyCode::Enter, simple(EditCommand::InsertNewline));
        shift_binding(keybindings, KeyCode::Enter, simple(EditCommand::InsertNewline));
    }
}
