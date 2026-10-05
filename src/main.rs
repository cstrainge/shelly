
use std::borrow::Cow;

use reedline::{ Color,
                Emacs,
                Keybindings,
                KeyModifiers,
                KeyCode,
                Prompt,
                PromptEditMode,
                PromptHistorySearch,
                Reedline,
                Signal,
                ReedlineEvent,
                EditCommand };


mod language;
mod runtime;


use language::interpreter::Interpreter;


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

    binding(keybindings, KeyCode::Left, ReedlineEvent::Left);
    binding(keybindings, KeyCode::Right, ReedlineEvent::Right);
    binding(keybindings, KeyCode::Up, ReedlineEvent::Up);
    binding(keybindings, KeyCode::Down, ReedlineEvent::Down);

    binding(keybindings, KeyCode::Backspace, simple(EditCommand::Backspace));
    binding(keybindings, KeyCode::Delete, simple(EditCommand::Delete));
    binding(keybindings, KeyCode::Home, simple(EditCommand::MoveToLineStart { select: false }));
    binding(keybindings, KeyCode::End, simple(EditCommand::MoveToLineEnd { select: false }));

    ctrl_binding(keybindings, KeyCode::Char('c'), ReedlineEvent::CtrlC);
    ctrl_binding(keybindings, KeyCode::Char('d'), ReedlineEvent::CtrlD);
    ctrl_binding(keybindings, KeyCode::Char('z'), simple(EditCommand::Undo));
    ctrl_binding(keybindings, KeyCode::Char('y'), simple(EditCommand::Redo));
    ctrl_binding(keybindings, KeyCode::Left, simple(EditCommand::MoveWordLeft { select: false }));
    ctrl_binding(keybindings, KeyCode::Right, simple(EditCommand::MoveWordRight { select: false }));
    ctrl_binding(keybindings, KeyCode::Backspace, simple(EditCommand::BackspaceWord));
    ctrl_binding(keybindings, KeyCode::Delete, simple(EditCommand::DeleteWord));

    ctrl_binding(keybindings, KeyCode::Enter, simple(EditCommand::InsertNewline));
    shift_binding(keybindings, KeyCode::Enter, simple(EditCommand::InsertNewline));
}


fn default_prompt() -> String
{
    let cwd = std::env::current_dir().unwrap_or_else(|_| ".".into());

    let formatted = format!("\n{} [{}]\n",
                            Color::Yellow.bold().paint("<shelly>"),
                            Color::Cyan.paint(cwd.display().to_string()));

    formatted
}


fn main()
{
    let mut interpreter = Interpreter::new();

    let mut prompt = ShellyPrompt { prompt_text: String::new() };
    let mut keybindings = Keybindings::empty();

    apply_keybindings(&mut keybindings);


    if let Some(home) = std::env::home_dir()
    {
        let init_path = home.join(".shelly_init.shy");

        if init_path.exists()
        {
            // Load the file to a string.
            if let Ok(contents) = std::fs::read_to_string(&init_path)
            {
                let result = interpreter.execute_code(init_path.to_str()
                                        .unwrap_or("<init>"), &contents);

                if let Err(error) = result
                {
                    println!("Error processing init file: {}", error);
                }
            }
        }
    }

    let mut editor = Reedline::create()
        .use_kitty_keyboard_enhancement(true)
        .with_edit_mode(Box::new(Emacs::new(keybindings)));

    loop
    {
        let prompt_text = if interpreter.has_command("prompt")
            {
                let (result, bytes) = interpreter.capture_stdout(|interpreter|
                    {
                        interpreter.execute_command(location_here!(),
                                                    "prompt",
                                                    vec![])
                    });

                if result.is_err()
                {
                    default_prompt()
                }
                else
                {
                    String::from_utf8_lossy(&bytes).to_string()
                }
            }
            else
            {
                default_prompt()
            };

        prompt.prompt_text = prompt_text;

        match editor.read_line(&prompt)
        {
            Ok(Signal::Success(text)) =>
                {
                    let result = interpreter.execute_code("<repl>", &text);

                    if let Err(error) = result
                    {
                        println!("Error processing text: {}", error);
                        continue;
                    }

                    if interpreter.halted
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
}
