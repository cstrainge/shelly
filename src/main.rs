
use std::{ fs::File, io::{ BufReader, IsTerminal, stdin, stderr }, path::PathBuf, process::ExitCode };

use clap::Parser;
use supports_color::Stream;

mod language;
mod runtime;

use crate::{ language::{ interpreter::{ Interpreter, Startup, Interactive, RcFile },
                         text::{ buffer::{ Buffer, SimpleBuffer }, read_buffer::ReadBuffer } },
             runtime::{ color::TtyColorMode,
                        repl::Repl,
                        result::{ RuntimeResult, RuntimeError } } };



#[derive(Parser, Debug)]
#[command(name = env!("CARGO_BIN_NAME"),
          version,
          about = "The Shelly interactive shell and scripting language.")]
struct CommandArguments
{
    /// Force interactive mode.
    #[arg(short = 'i', conflicts_with_all = ["stdin_code"])]
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

    /// Arguments passed to the script, ignored by the shell itself.
    #[arg(value_name = "script & arguments", trailing_var_arg = true, allow_hyphen_values = true)]
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
    Script(PathBuf),

    /**
     * Are we executing code passed via the command line and then exiting?
     */
    Code(String),

    /**
     * Are we reading source code from standard input and then exiting?
     */
    Stdin
}


/**
 * Based on the command line arguments and the terminal, (if any,) we're attached to determine how
 * the shell should run.
 */
fn determine_running_mode(args: &CommandArguments) -> (RunningMode, Vec<String>)
{
    // Did the user force interactive mode?
    if args.interactive
    {
        return (RunningMode::Interactive, args.script_arguments.clone());
    }

    // Or are we reading the source code for a script from standard input?
    if args.stdin_code
    {
        return (RunningMode::Stdin, args.script_arguments.clone());
    }

    // Or are we executing code passed via the command line?
    if args.code.is_some()
    {
        return (RunningMode::Code(args.code.as_ref().unwrap().clone()), args.script_arguments.clone());
    }

    // Or are we executing a script?
    //if args.script.is_some()
    //{
    //    return RunningMode::Script(args.script.as_ref().unwrap().clone());
    //}

    if args.script_arguments.len() >= 1
    {
        return (RunningMode::Script(PathBuf::from(&args.script_arguments[0])),
                args.script_arguments[1..].to_vec());
    }

    // Now determine if we're running in an interactive tty.
    if    stdin().is_terminal()
       && stderr().is_terminal()
    {
        return (RunningMode::Interactive, args.script_arguments.clone());
    }

    // Default to reading from standard input.
    (RunningMode::Stdin, args.script_arguments.clone())
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
               suppress_banner: bool,
               rc_file: RcFile,
               script_args: Vec<String>) -> RuntimeResult<ExitCode>
{
    let mut repl = Repl::new(color_mode, tab_width, startup, suppress_banner, rc_file, script_args);

    repl.run()
}


/**
 * Interpret the source code from the given buffer using the interpreter.
 */
fn interpret(buffer: &mut dyn Buffer,
             startup: Startup,
             color_mode: TtyColorMode,
             tab_width: usize,
             script_args: Vec<String>) -> RuntimeResult<ExitCode>
{
    let mut interpreter = Interpreter::new(startup,
                                           Interactive::No,
                                           color_mode,
                                           tab_width,
                                           RcFile::None,
                                           script_args);

    interpreter.execute_from_buffer(buffer)?;

    Ok(ExitCode::from(interpreter.exit_code))
}


/**
 * Run the script file as specified by the command-line arguments.
 */
fn run_script(script: &PathBuf,
              tab_width: usize,
              color_mode: TtyColorMode,
              startup: Startup,
              script_args: Vec<String>) -> RuntimeResult<ExitCode>
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

    interpret(&mut buffer, startup, color_mode, tab_width, script_args)
}


/**
 * Run the code as specified in the command line arguments.
 */
fn run_code(code: &String,
            tab_width: usize,
            color_mode: TtyColorMode,
            startup: Startup,
            script_args: Vec<String>) -> RuntimeResult<ExitCode>
{
    let mut buffer = SimpleBuffer::new("command line", &code, Some(tab_width));

    interpret(&mut buffer, startup, color_mode, tab_width, script_args)
}


/**
 * Run the code that's streaming in from stdin.
 */
fn run_stdin(tab_width: usize,
             color_mode: TtyColorMode,
             startup: Startup,
             script_args: Vec<String>) -> RuntimeResult<ExitCode>
{
    let mut buffer = BufReader::new(stdin());
    let mut buffer = ReadBuffer::new("standard input", &mut buffer, Some(tab_width));

    interpret(&mut buffer, startup, color_mode, tab_width, script_args)
}


/**
 * Process the command line arguments and determine the mode we are running in.
 */
fn main() -> RuntimeResult<ExitCode>
{
    let args = CommandArguments::parse();

    if let Some(tab_width) = args.tab_width && tab_width == 0
    {
        return Err(RuntimeError::InvalidTabWidth);
    }

    let color_mode = determine_color_mode(&args);

    let invoked_as_login = std::env::args_os()
        .next()
        .is_some_and(|name| name.as_encoded_bytes().starts_with(b"-"));

    let rc_file = if let Some(custom_rc) = &args.rcfile
        {
            RcFile::Custom(custom_rc.clone())
        }
        else if args.norc
        {
            RcFile::None
        }
        else
        {
            RcFile::Default
        };

    let startup = if args.login || invoked_as_login
        {
            Startup::Login
        }
        else
        {
            Startup::NonLogin
        };

    let tab_width = args.tab_width.unwrap_or(4);
    let (mode, script_args) = determine_running_mode(&args);

    match mode
    {
        RunningMode::Interactive => run_as_repl(tab_width,
                                                color_mode,
                                                startup,
                                                args.no_banner,
                                                rc_file,
                                                script_args),

        RunningMode::Script(path) => run_script(&path,
                                                tab_width,
                                                color_mode,
                                                startup,
                                                script_args),

        RunningMode::Code(code)   => run_code(&code,
                                              tab_width,
                                              color_mode,
                                              startup,
                                              script_args),

        RunningMode::Stdin        => run_stdin(tab_width,
                                               color_mode,
                                               startup,
                                               script_args)
    }
}
