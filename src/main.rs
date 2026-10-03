
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


use language::{ ast::AstStatement,
                text::buffer::SimpleBuffer,
                parser::{ ParserError, parse_text },
                tokenizer::{ Tokenizer,
                             TokenKind,
                             TokenValue } };


struct ShellyPrompt
{
}

impl Prompt for ShellyPrompt
{
    fn render_prompt_left(&self) -> Cow<'_, str>
    {
        let formatted = format!("\n{}\n{}\n",
                                Color::Yellow.bold().paint("[git: main]"),
                                Color::LightBlue.bold().paint("~/workdir/foo/"));

        Cow::Owned(formatted)
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


fn process(text: &str) -> Result<bool, ParserError>
{
    let mut buffer = SimpleBuffer::new("<repl>", text, None);
    let mut tokenizer = Tokenizer::new(&mut buffer);
    let statements = parse_text(&mut tokenizer)?;

    let mut should_continue = true;

    for statement in statements
    {
        let AstStatement::ExecuteStatement(execute_statement) = statement;

        if execute_statement.executable_name == "exit"
        {
            should_continue = false;
        }

        println!("Execute command {}.", execute_statement.executable_name);
    }

    Ok(should_continue)
}


fn main()
{
    let prompt = ShellyPrompt { };
    let mut keybindings = Keybindings::empty();

    apply_keybindings(&mut keybindings);

    let mut editor = Reedline::create()
        .use_kitty_keyboard_enhancement(true)
        .with_edit_mode(Box::new(Emacs::new(keybindings)));

    loop
    {
        match editor.read_line(&prompt)
        {
            Ok(Signal::Success(text)) =>
                {
                    let result = process(&text);

                    if let Err(error) = result
                    {
                        println!("Error processing text: {}", error);
                        continue;
                    }

                    if    let Ok(should_continue) = result
                       && should_continue == false
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
