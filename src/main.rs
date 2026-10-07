
use std::{ fs::File, io::{ BufReader, IsTerminal, stdin, stderr }, path::PathBuf };

use clap::Parser;
use supports_color::Stream;

mod language;
mod runtime;

use crate::{ language::{ interpreter::{ Interpreter, Startup, Interactive },
                         text::{ buffer::{ Buffer, SimpleBuffer }, read_buffer::ReadBuffer } },
             runtime::{ color::TtyColorMode,
                        repl::Repl,
                        result::{ RuntimeResult, RuntimeError } } };



#[derive(Parser, Debug)]
#[command(version, about = "The Shelly interactive shell and scripting language.")]
struct CommandArguments
{
    /// Force interactive mode.
    #[arg(short = 'i', conflicts_with_all = ["script", "stdin_code"])]
    interactive: bool,

    /// Start as a login shell.
    #[arg(short = 'l', long)]
    login: bool,

    /// Execute the specified code and exit.
    #[arg(short = 'c', long,
          value_name = "code",
          conflicts_with_all = ["stdin_code", "interactive"])]
    code: Option<String>,

    /// Read source code from standard input; remaining arguments are passed to the script.
    #[arg(short = 's', long = "stdin")]
    stdin_code: bool,

    /// Do not show the shell's startup banner.
    #[arg(short = 'b', long)]
    no_banner: bool,

    /// Force monochrome mode.
    #[arg(short = 'm', long = "mono")]
    mono: bool,

    /// Specify the width that the shell should read tab characters as.
    #[arg(short = 't', long)]
    tab_width: Option<usize>,

    /// Do not load the shell's configuration file.
    #[arg(long)]
    norc: bool,

    /// Specify an alternative configuration file to load.
    #[arg(long, conflicts_with = "norc")]
    rcfile: Option<PathBuf>,

    /// Execute the specified script.
    #[arg(value_name = "SCRIPT", conflicts_with_all = ["code", "stdin_code"])]
    script: Option<PathBuf>,

    /// Arguments passed to the script, ignored by the shell itself.
    #[arg(value_name = "script arguments", last = true)]
     script_arguments: Vec<String>,
}


/**
 * The shell's execution mode, determined from command-line arguments and the system environment.
 */
enum RunningMode
{
    /**
     * Are we running the full REPL?
     */
    Interactive,

    /**
     * Are we executing a script and then exiting?
     */
    Script,

    /**
     * Are we executing code passed via the command line and then exiting?
     */
    Code,

    /**
     * Are we reading source code from standard input and then exiting?
     */
    Stdin
}


/**
 * Based on the command line arguments and the terminal, (if any,) we're attached to determine how
 * the shell should run.
 */
fn determine_running_mode(args: &CommandArguments) -> RunningMode
{
    // Did the user force interactive mode?
    if args.interactive
    {
        return RunningMode::Interactive;
    }

    // Or are we reading the source code for a script from standard input?
    if args.stdin_code
    {
        return RunningMode::Stdin;
    }

    // Or are we executing code passed via the command line?
    if args.code.is_some()
    {
        return RunningMode::Code;
    }

    // Or are we executing a script?
    if args.script.is_some()
    {
        return RunningMode::Script;
    }

    // Now determine if we're running in an interactive tty.
    if    stdin().is_terminal()
       && stdin().is_terminal()
       && stderr().is_terminal()
    {
        return RunningMode::Interactive;
    }

    // Default to reading from standard input.
    RunningMode::Stdin
}


/**
 * Determine the color mode for the shell based on command-line arguments and environment.
 */
fn determine_color_mode(args: &CommandArguments) -> TtyColorMode
{
    if args.mono
    {
        return TtyColorMode::TtyMonochrome;
    }

    match supports_color::on(Stream::Stdout)
    {
        Some(c) if c.has_16m   => TtyColorMode::TtyTrueColor,
        Some(c) if c.has_256   => TtyColorMode::Tty256,
        Some(c) if c.has_basic => TtyColorMode::TtyBasic,
        _                      => TtyColorMode::TtyMonochrome
    }
}


/**
 * Run the shell as a REPL, executing commands interactively.
 */
fn run_as_repl(tab_width: usize,
               color_mode: TtyColorMode,
               startup: Startup,
               suppress_banner: bool) -> RuntimeResult<()>
{
    let mut repl = Repl::new(color_mode, tab_width, startup, suppress_banner);

    repl.run()
}


/**
 * Interpret the source code from the given buffer using the interpreter.
 */
fn interpret(buffer: &mut dyn Buffer,
             startup: Startup,
             color_mode: TtyColorMode,
             tab_width: usize) -> RuntimeResult<()>
{
    let mut interpreter = Interpreter::new(startup, Interactive::No, color_mode, tab_width);

    interpreter.execute_from_buffer(buffer)?;

    Ok(())
}


/**
 * Run the script file as specified by the command-line arguments.
 */
fn run_script(script: &PathBuf,
              tab_width: usize,
              color_mode: TtyColorMode,
              startup: Startup) -> RuntimeResult<()>
{
    let file = File::open(script);

    if let Err(e) = file
    {
        return Err(RuntimeError::FileOpenError(script.clone(), e));
    }

    let file = file.unwrap();
    let script = script.to_str().unwrap_or("input script");
    let mut file_buffer = BufReader::new(file);
    let mut buffer = ReadBuffer::new(&script, &mut file_buffer, Some(tab_width));

    interpret(&mut buffer, startup, color_mode, tab_width)
}


/**
 * Run the code as specified in the command line arguments.
 */
fn run_code(code: &String,
            tab_width: usize,
            color_mode: TtyColorMode,
            startup: Startup) -> RuntimeResult<()>
{
    let mut buffer = SimpleBuffer::new("command line", &code, Some(tab_width));

    interpret(&mut buffer, startup, color_mode, tab_width)
}


/**
 * Run the code that's streaming in from stdin.
 */
fn run_stdin(tab_width: usize,
             color_mode: TtyColorMode,
             startup: Startup) -> RuntimeResult<()>
{
    let mut buffer = BufReader::new(stdin());
    let mut buffer = ReadBuffer::new("standard input", &mut buffer, Some(tab_width));

    interpret(&mut buffer, startup, color_mode, tab_width)
}


/**
 * Process the command line arguments and determine the mode we are running in.
 */
fn main() -> RuntimeResult<()>
{
    let args = CommandArguments::parse();
    let color_mode = determine_color_mode(&args);
    let startup = if args.login
        {
            Startup::Login
        }
        else
        {
            Startup::NonLogin
        };

    let tab_width = args.tab_width.unwrap_or(4);

    match determine_running_mode(&args)
    {
        RunningMode::Interactive => run_as_repl(tab_width, color_mode, startup, args.no_banner),

        RunningMode::Script      => run_script(&args.script.unwrap(),
                                               tab_width,
                                               color_mode,
                                               startup),

        RunningMode::Code        => run_code(&args.code.unwrap(), tab_width, color_mode, startup),

        RunningMode::Stdin       => run_stdin(tab_width, color_mode, startup)
    }
}
