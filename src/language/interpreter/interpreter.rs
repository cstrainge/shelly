
use std::{ cell::RefCell,
           collections::{ HashMap, HashSet, VecDeque },
           env::{ consts::OS,
                  current_dir,
                  current_exe,
                  home_dir,
                  join_paths,
                  set_current_dir,
                  split_paths },
           fmt::{ self, Debug, Display, Formatter },
           panic::Location as NativeLocation,
           fs::File,
           io::{ BufReader, Error, ErrorKind, Read, Write, copy, pipe, stdout, stderr },
           path::{ Path, PathBuf, Component, MAIN_SEPARATOR_STR, is_separator },
           process::{ Command, Stdio, id },
           rc::Rc,
           mem::{ replace, take },
           thread::Builder,
           time::Duration };

use glob::{ MatchOptions, Pattern, glob };

use crossterm::event::Event;

use hostname::get as get_hostname;

use crate::{
    language::{
        bytecode::{Code, Instruction, FunctionRef},
        compiler::CompileError,
        interpreter::CallFrame,
        native::{NativeFunction, NativeFunctions, NativeVisibility, NativeOutcome},
        data::{
            value::{ExecResult, Executable, Value},
            closure::ClosureValue,
            formats::{FACTORIES, encode, decode},
            map_key::MapKey,
            methods::{BoundMethod, MethodDefinition, method_key},
            range::Range,
            conversion::is_conversion_method,
            scoped_variables::{
                PropertyReference, ScopedValue, ScopedVariables, ValueReference, ValueVisibility,
            },
            types::{StructValue, TypeId, TypeKind, TypeRegistry},
        },
        parser::{ParserError, parse_text},
        tokenizer::Tokenizer,
        text::{
            interpolation::interpolation_references, buffer::Buffer, location::Location,
            read_buffer::ReadBuffer,
        },
        interpreter::{
            Alias,
            iteration::Iteration,
            scope::Scope,
            modules::{Prelude, ModuleSource, PendingModule},
            redirection::{Redirection, Output, configure},
        },
    },
    runtime::{
        color::TtyColorMode,
        process::{COMMANDS, invoke},
        terminal::detect_capabilities,
        sixel::banner as sixel_banner,
        iterm2::banner as iterm2_banner,
        signals::take_interrupt,
    },
};

const MAIN_SCOPE: &str = "main";

const BANNER_TRUECOLOR: &str = include_str!("../../../banner_truecolor.txt");
const BANNER_256: &str = include_str!("../../../banner_256.txt");
const BANNER_MONO: &str = include_str!("../../../banner_mono.txt");
const BANNER_SIXEL: &str = include_str!("../../../banner_sixel.txt");


struct LoopFrame
{
    continue_target: usize,
    break_target: usize,
    scope: usize,
    stack_depth: usize,
    iteration_depth: usize,
    reference_depth: usize,
    redirection_depth: usize,
    failure_depth: usize,
}

#[derive(Clone)]
pub enum ErrorWhat
{
    Raised(String),
    CleanupTimeout,
    HookTimeout,
    ReceiverBusy(String),
    OperationCancelled,
    CleanupAbort,
    CleanupControl,
    UnsupportedSuspension(String),
    ParserError(ParserError),
    CompileError(CompileError),
    InvalidOperand(String),
    ArithmeticError(String),
    ArrayError(String),
    HashMapError(String),
    RangeError(String),
    MatchError,
    ModuleError(String),
    IterationError(String),
    LoopControlError(String),
    ArgumentMismatch(String),
    ReturnContract(String, usize),
    FileGlobError(String),
    StackUnderflow,
    NoResult,
    ReturnOutsideFunction,
    ExecutableNotFound(String),
    ExecutableIoError(String),
    RedirectionError(String),
    ExecutableBadReturn(u8),
    ExecutableSignaled,
    Interrupted,
    InitialScopePopAttempt,
}


impl Display for ErrorWhat
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result
    {
        match self
        {
            ErrorWhat::Raised(message) => write!(f, "{}", message),
            ErrorWhat::OperationCancelled => write!(f,
                "Operation was cancelled or exceeded its deadline."),
            ErrorWhat::ReceiverBusy(message) => write!(f, "{}", message),
            ErrorWhat::HookTimeout => write!(f, "Conversion hook budget exhausted."),
            ErrorWhat::CleanupTimeout => write!(f, "Cleanup budget exhausted."),
            ErrorWhat::CleanupAbort => write!(f, "Cleanup aborted by another interruption."),
            ErrorWhat::CleanupControl => write!(f, "Control flow cannot escape finally."),
            ErrorWhat::UnsupportedSuspension(message) => write!(f, "{}", message),
            ErrorWhat::ParserError(error) => write!(f, "Parser error: {}.", error),
            ErrorWhat::CompileError(error) => write!(f, "Compile error: {}.", error),
            ErrorWhat::InvalidOperand(message) => write!(f, "Invalid operand: {}.", message),
            ErrorWhat::ArithmeticError(message) => write!(f, "Arithmetic error: {}.", message),
            ErrorWhat::ArrayError(message) => write!(f, "Array error: {}.", message),
            ErrorWhat::HashMapError(message) => write!(f, "Hash map error: {}.", message),
            ErrorWhat::RangeError(message) => write!(f, "Range error: {}.", message),
            ErrorWhat::IterationError(message) => write!(f, "Iteration error: {}.", message),
            ErrorWhat::LoopControlError(message) => write!(f, "Loop control error: {}.", message),
            ErrorWhat::FileGlobError(message) => write!(f, "File glob error: {}.", message),
            ErrorWhat::StackUnderflow => write!(f, "Stack underflow"),
            ErrorWhat::NoResult => write!(
                f,
                "Attempted to access the last result, but there was none."
            ),
            ErrorWhat::ReturnOutsideFunction => write!(f, "Cannot return outside a function."),
            ErrorWhat::ExecutableNotFound(name) => write!(f, "Executable not found: {}.", name),
            ErrorWhat::ExecutableIoError(message) =>
                write!(f, "Executable I/O error: {}.", message),
            ErrorWhat::RedirectionError(message) => write!(f, "Redirection error: {}.", message),
            ErrorWhat::ModuleError(message) => write!(f, "Module error: {}.", message),
            ErrorWhat::MatchError => write!(f, "Match error: No arm matched the value."),
            ErrorWhat::ExecutableBadReturn(code) =>
                {
                    write!(f, "Executable returned error code: {}.", code)
                },

            ErrorWhat::ExecutableSignaled =>
                {
                    write!(f, "Executable was terminated by a signal.")
                },

            ErrorWhat::ArgumentMismatch(message) => write!(f, "{}", message),
            ErrorWhat::ReturnContract(message, _) => write!(f, "{}", message),
            ErrorWhat::Interrupted => write!(f, "Execution interrupted."),
            ErrorWhat::InitialScopePopAttempt => write!(f, "Attempted to pop the initial scope.")
        }
    }
}


#[derive(Clone)]
pub struct InterpreterError
{
    pub(super) location: Location,
    pub(super) what: ErrorWhat,
    pub(super) trace: Box<ErrorTrace>,
}

#[derive(Clone)]
pub(super) struct ErrorTrace
{
    pub(super) abort_cleanup: bool,
    pub(super) cleanup: Option<crate::language::interpreter::exceptions::CleanupGuard>,
    pub(super) raised: Option<Value>,
    pub(super) related: Vec<crate::language::data::failure::RelatedFailure>,
    pub(super) resolved: Vec<crate::language::data::failure::FailureRef>,
    pub(super) rethrow_source: Option<u64>,
    pub(super) call_stack: Vec<CallFrame>,
    pub(super) native_location: Option<Location>,
}


impl InterpreterError
{
    #[track_caller]
    pub(super) fn new(location: Location, what: ErrorWhat) -> Self
    {
        let caller = NativeLocation::caller();
        let native_location = if matches!(what,
            ErrorWhat::ParserError(_) | ErrorWhat::CompileError(_))
        {
            None
        }
        else
        {
            Some(Location::new(caller.file(), caller.line() as usize,
                               caller.column() as usize))
        };
        let trace = Box::new(ErrorTrace
            {
                call_stack: Vec::new(),
                raised: None, related: Vec::new(), resolved: Vec::new(), rethrow_source: None,
                cleanup: None,
                abort_cleanup: matches!(what, ErrorWhat::CleanupAbort | ErrorWhat::CleanupTimeout),
                native_location,
            });
        Self { location, what, trace }
    }

    pub(super) fn category(&self) -> &str
    {
        match self.what
        {
            ErrorWhat::Raised(_) => "Raised",
            ErrorWhat::Interrupted => "Interrupted",
            ErrorWhat::CleanupTimeout => "CleanupTimeout",
            ErrorWhat::HookTimeout => "BudgetExceeded",
            ErrorWhat::ReceiverBusy(_) => "ReceiverBusy",
            ErrorWhat::OperationCancelled => "CancelledError",
            ErrorWhat::CleanupAbort => "CleanupAbort",
            ErrorWhat::CleanupControl => "CleanupControlError",
            ErrorWhat::UnsupportedSuspension(_) => "UnsupportedSuspension",
            ErrorWhat::ArithmeticError(_) => "ArithmeticError",
            ErrorWhat::ArrayError(_) | ErrorWhat::HashMapError(_) => "BoundsError",
            ErrorWhat::IterationError(_) => "IterationError",
            ErrorWhat::ModuleError(_) => "ModuleError",
            ErrorWhat::ArgumentMismatch(_) => "ArgumentMismatch",
            ErrorWhat::ReturnContract(_, _) => "ReturnContractError",
            ErrorWhat::ExecutableNotFound(_) => "ExecutableNotFound",
            ErrorWhat::ExecutableIoError(_) => "ExecutableIoError",
            ErrorWhat::ExecutableBadReturn(_) => "ExecutableBadReturn",
            ErrorWhat::ExecutableSignaled => "ExecutableSignaled",
            ErrorWhat::RedirectionError(_) => "RedirectionError",
            _ => "RuntimeError",
        }
    }

    pub fn call_stack(&self) -> &[CallFrame] { &self.trace.call_stack }
    pub fn was_interrupted(&self) -> bool
    {
        matches!(self.what, ErrorWhat::Interrupted | ErrorWhat::CleanupAbort)
    }

    pub fn native_location(&self) -> Option<&Location> { self.trace.native_location.as_ref() }

    pub(super) fn with_context(self, mut context: Self) -> Self
    {
        context.location = self.location;
        context.trace.native_location = self.trace.native_location;
        context.trace.call_stack = self.trace.call_stack;
        context.trace.raised = self.trace.raised;
        context.trace.related = self.trace.related;
        context.trace.resolved = self.trace.resolved;
        context.trace.rethrow_source = self.trace.rethrow_source;
        context.trace.cleanup = self.trace.cleanup;
        context.trace.abort_cleanup |= self.trace.abort_cleanup;
        context
    }

    #[track_caller]
    pub(super) fn through_call(mut self, name: &str, location: &Location, native: bool) -> Self
    {
        if self.trace.raised.is_some()
        {
            return self;
        }
        let caller = NativeLocation::caller();
        let native_location = native.then(||
            Location::new(caller.file(), caller.line() as usize, caller.column() as usize));
        self.trace.call_stack.push(CallFrame
            {
                name: name.into(),
                location: location.clone(),
                native,
                native_location,
            });
        self
    }
}


impl From<ParserError> for InterpreterError
{
    fn from(error: ParserError) -> Self
    {
        InterpreterError::new(Location::default(), ErrorWhat::ParserError(error))
    }
}


impl From<CompileError> for InterpreterError
{
    fn from(error: CompileError) -> Self
    {
        InterpreterError::new(Location::default(), ErrorWhat::CompileError(error))
    }
}


impl Display for InterpreterError
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result
    {
        write!(f, "Interpreter error at {}: {}", self.location, self.what)?;
        if let Some(location) = self.native_location()
        { write!(f, "\nNative error at {}", location)?; }
        if !self.trace.call_stack.is_empty()
        {
            write!(f, "\nCall stack (most recent call first):")?;
            for frame in self.call_stack()
            {
                let native = if frame.native { " [native]" } else { "" };
                write!(f, "\n    at {}{} ({})", frame.name, native, frame.location)?;
                if let Some(location) = &frame.native_location
                { write!(f, "\n        native boundary at {}", location)?; }
            }
        }
        Ok(())
    }
}


impl Debug for InterpreterError
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result
    {
        write!(f, "{}", self)
    }
}


pub type BuiltIn<'a> = Rc<dyn Fn(&mut Interpreter,
                                 &Location,
                                 &[Value]) -> InterpreterResult<()> + 'a>;

pub type InterpreterResult<T> = Result<T, InterpreterError>;



type ReadFunction = Rc<dyn Fn(&Interpreter) -> InterpreterResult<Value>>;


type SpecialVars = HashMap<&'static str, ReadFunction>;


pub enum Startup
{
    Login,
    NonLogin
}


pub enum Interactive
{
    Yes,
    YesWithoutBanner,
    No
}


pub struct Interpreter
{
    pub(super) interactive: bool,
    pub(super) reactor: Option<Rc<crate::runtime::tasks::Reactor>>,
    pub(super) exceptions: crate::language::interpreter::exceptions::ExceptionState,
    protocol_context: Vec<(Option<TypeId>, String)>,
    active_factories: Vec<TypeId>,
    pub(super) scopes: HashMap<String, Scope>,
    pub(super) current_scope: String,
    pub(super) loading_modules: HashSet<String>,
    pub(super) module_sources: HashMap<String, ModuleSource>,
    pub(super) pending_modules: HashMap<String, PendingModule>,
    pub(super) prelude: Option<Prelude>,
    pub(super) prelude_loading: bool,
    pub(super) prelude_generation: usize,
    // A source file can define several generations of nominal types after reloads.
    pub(super) type_scopes: HashMap<TypeId, String>,
    special_vars: SpecialVars,
    pub(super) native_functions: NativeFunctions,
    pub(super) widget_environment: Option<Vec<(String, String)>>,
    pub(super) prompt_widgets: Option<Rc<RefCell<ScopedValue>>>,
    pub(super) pipeline_inputs: Vec<Value>,
    pub(super) captured_stdout: Option<Vec<u8>>,
    pub(super) redirections: Vec<Redirection>,
    last_cmd_time: Duration,
    pub(super) color_mode: TtyColorMode,
    startup_input: Vec<Event>,
    pub last_result: Option<Value>,
    pub exit_code: u8,
    pub halted: bool,
}


#[derive(Clone, PartialEq, Eq)]
pub enum RcFile
{
    None,
    Default,
    Custom(PathBuf)
}


impl Interpreter
{
    pub(super) fn fork_task(&self) -> Self
    {
        Self {
            interactive: self.interactive,
            reactor: None,
            exceptions: Default::default(),
            protocol_context: self.protocol_context.clone(),
            active_factories: Vec::new(),
            scopes: self
                .scopes
                .iter()
                .map(|(name, scope)| (name.clone(), scope.checkpoint()))
                .collect(),
            current_scope: self.current_scope.clone(),
            loading_modules: self.loading_modules.clone(),
            module_sources: self.module_sources.clone(),
            pending_modules: HashMap::new(),
            prelude: self.prelude.clone(),
            prelude_loading: false,
            prelude_generation: self.prelude_generation,
            type_scopes: self.type_scopes.clone(),
            special_vars: self.special_vars.clone(),
            native_functions: self.native_functions.clone(),
            widget_environment: None,
            prompt_widgets: None,
            pipeline_inputs: Vec::new(),
            captured_stdout: None,
            redirections: Vec::new(),
            last_cmd_time: Duration::default(),
            color_mode: self.color_mode,
            startup_input: Vec::new(),
            last_result: None,
            exit_code: 0,
            halted: false,
        }
    }
    pub fn take_startup_input(&mut self) -> Vec<Event>
    {
        take(&mut self.startup_input)
    }

    pub(super) fn scope(&self) -> &Scope
    {
        self.scopes.get(&self.current_scope)
            .expect("The current scope must be registered.")
    }

    pub(super) fn scope_mut(&mut self) -> &mut Scope
    {
        self.scopes.get_mut(&self.current_scope)
            .expect("The current scope must be registered.")
    }

    pub(super) fn exported_environment(&self, location: &Location)
        -> InterpreterResult<Vec<(String, String)>>
    {
        if let Some(environment) = &self.widget_environment { return Ok(environment.clone()); }
        self.scope().variables.get_all_flattened()
            .iter()
            .filter(|(_, value)| value.exported == ValueVisibility::Exported)
            .map(|(key, _)|
                {
                    let text = self.read_raw_variable(key, location)?.as_text();
                    let text = if key == "$PATH"
                        {
                            self.eval_path_list_from(&text)
                        }
                        else
                        {
                            self.eval_path_from(&text)
                        };

                    Ok((key.strip_prefix('$').unwrap_or(key.as_str()).to_string(), text))
                })
            .collect()
    }

    pub fn new(startup: Startup,
               interactive: Interactive,
               color_mode: TtyColorMode,
               tab_width: usize,
               rc_file: RcFile,
               script_args: Vec<String>) -> Self
    {
        let mut interpreter = Self::registered(color_mode);
        interpreter.initialize_startup(startup, interactive, color_mode, tab_width,
                                       rc_file, script_args);
        interpreter
    }

    // Registration alone is safe for tooling; startup can execute scripts and is separate.
    pub(super) fn registered(color_mode: TtyColorMode) -> Self
    {
        let mut bodies: HashMap<&'static str, BuiltIn<'static>> = HashMap::from([
                (
                    "cd",
                    Rc::new(Interpreter::handle_cd) as BuiltIn<'static>
                ),

                (
                    "visible",
                    Rc::new(Interpreter::handle_visible) as BuiltIn<'static>
                ),

                (
                    "prelude_reload",
                    Rc::new(Interpreter::handle_prelude_reload) as BuiltIn<'static>
                ),

                (
                    "exit",
                    Rc::new(Interpreter::handle_exit) as BuiltIn<'static>
                )
            ]);

        for &name in COMMANDS
        {
            bodies.insert(name, Rc::new(move |interpreter, location, args|
                {
                    let environment = interpreter.exported_environment(location)?;
                    let mut capture = None;
                    let interruption = crate::runtime::signals::generation();
                    let result = invoke(name, args, &environment,
                        |path| interpreter.eval_path_from(path),
                        |command|
                            {
                                let redirected = configure(command, &interpreter.redirections)?;
                                if interpreter.captured_stdout.is_some() && !redirected
                                {
                                    let (mut reader, writer) = pipe()?;
                                    capture = Some(Builder::new().name("shelly-process".into())
                                        .spawn(move ||
                                            {
                                                let mut bytes = Vec::new();
                                                reader.read_to_end(&mut bytes)?;
                                                Ok::<_, Error>(bytes)
                                            })?);
                                    command.stdout(writer);
                                }
                                Ok(())
                            });
                    if let Some(worker) = capture
                    {
                        let bytes = worker.join()
                            .map_err(|_| Error::other("Process capture worker panicked"))
                            .and_then(|result| result)
                            .map_err(|error| Self::redirection_error(location, error))?;
                        if let Some(output) = interpreter.captured_stdout.as_mut()
                        { output.extend_from_slice(&bytes); }
                    }
                    if !interpreter.interactive && crate::runtime::signals::generation() !=
                        interruption
                    {
                        take_interrupt();
                        return Err(InterpreterError::new(location.clone(), ErrorWhat::Interrupted));
                    }
                    let result = result.map_err(|error| InterpreterError::new(
                        location.clone(),
                        ErrorWhat::InvalidOperand(error)
                    ))?;
                    interpreter.last_result = Some(result);
                    Ok(())
                }));
        }

        for &(name, format, encoding) in FACTORIES
        {
            bodies.insert(name, Rc::new(move |interpreter, location, args|
                {
                    let [value] = args else
                    {
                        return Err(InterpreterError::new(
                            location.clone(),
                            ErrorWhat::ArgumentMismatch(
                                format!("{} requires exactly one argument", name)
                            )
                        ));
                    };
                    let result = if encoding
                        { encode(format, value) }
                        else { decode(format, value) };
                    interpreter.last_result = Some(result.map_err(|message| InterpreterError::new(
                        location.clone(),
                        ErrorWhat::InvalidOperand(message)
                    ))?);
                    Ok(())
                }));
        }

        let mut native_functions: NativeFunctions = bodies.into_iter().map(|(name, body)|
            (name, NativeFunction::new(name, NativeVisibility::Visible, body))).collect();
        native_functions.insert(
            "__report_error",
            NativeFunction::new(
                "__report_error",
                NativeVisibility::Hidden,
                Rc::new(Interpreter::handle_report_error),
            ),
        );
        native_functions.insert(
            "__json",
            NativeFunction::new(
                "__json",
                NativeVisibility::Hidden,
                Rc::new(Interpreter::handle_json),
            ),
        );
        native_functions.insert(
            "__json_bind",
            NativeFunction::new(
                "__json_bind",
                NativeVisibility::Hidden,
                Rc::new(Interpreter::handle_json_bind),
            ),
        );
        native_functions.insert(
            "__json_apply",
            NativeFunction::new(
                "__json_apply",
                NativeVisibility::Hidden,
                Rc::new(Interpreter::handle_json_apply),
            ),
        );
        native_functions.insert(
            "__json_reader",
            NativeFunction::suspending(
                "__json_reader",
                NativeVisibility::Hidden,
                Self::handle_json_reader,
            ),
        );
        native_functions.insert(
            "__json_source",
            NativeFunction::suspending(
                "__json_source",
                NativeVisibility::Hidden,
                Interpreter::handle_json_source,
            ),
        );
        native_functions.insert(
            "__stream",
            NativeFunction::new(
                "__stream",
                NativeVisibility::Hidden,
                Rc::new(Interpreter::handle_stream),
            ),
        );
        native_functions.insert(
            "__bytes",
            NativeFunction::suspending(
                "__bytes",
                NativeVisibility::Hidden,
                Interpreter::handle_bytes,
            ),
        );
        native_functions.insert(
            "__io",
            NativeFunction::suspending("__io", NativeVisibility::Hidden, Interpreter::handle_io),
        );
        native_functions.insert(
            "__io_close",
            NativeFunction::new(
                "__io_close",
                NativeVisibility::Hidden,
                Rc::new(Interpreter::handle_io_close),
            ),
        );
        native_functions.insert(
            "__socket",
            NativeFunction::suspending("__socket", NativeVisibility::Hidden, Self::handle_socket),
        );
        native_functions.insert(
            "__socket_close",
            NativeFunction::new(
                "__socket_close",
                NativeVisibility::Hidden,
                Rc::new(Self::handle_socket_close),
            ),
        );
        native_functions.insert(
            "__tcp",
            NativeFunction::suspending("__tcp", NativeVisibility::Hidden, Interpreter::handle_tcp),
        );
        native_functions.insert(
            "__tcp_close",
            NativeFunction::new(
                "__tcp_close",
                NativeVisibility::Hidden,
                Rc::new(Interpreter::handle_tcp_close),
            ),
        );
        native_functions.insert(
            "__http_upload",
            NativeFunction::suspending(
                "__http_upload",
                NativeVisibility::Hidden,
                Self::handle_http_upload,
            ),
        );
        native_functions.insert(
            "__server",
            NativeFunction::suspending("__server", NativeVisibility::Hidden, Self::handle_server),
        );
        native_functions.insert(
            "__server_close",
            NativeFunction::suspending(
                "__server_close",
                NativeVisibility::Hidden,
                Self::handle_server_close,
            ),
        );
        native_functions.insert(
            "__server_invoke",
            NativeFunction::new(
                "__server_invoke",
                NativeVisibility::Hidden,
                Rc::new(Self::handle_server_invoke),
            ),
        );
        native_functions.insert(
            "__http_apply",
            NativeFunction::new(
                "__http_apply",
                NativeVisibility::Hidden,
                Rc::new(Self::handle_http_apply),
            ),
        );
        native_functions.insert(
            "__http",
            NativeFunction::suspending(
                "__http",
                NativeVisibility::Hidden,
                Interpreter::handle_http,
            ),
        );
        native_functions.insert(
            "__http_close",
            NativeFunction::new(
                "__http_close",
                NativeVisibility::Hidden,
                Rc::new(Interpreter::handle_http_close),
            ),
        );
        native_functions.insert(
            "__http_header",
            NativeFunction::new(
                "__http_header",
                NativeVisibility::Hidden,
                Rc::new(Interpreter::handle_http_header),
            ),
        );
        native_functions.insert("__source_spans", NativeFunction::new("__source_spans",
            NativeVisibility::Hidden, Rc::new(Interpreter::handle_source_spans)));
        native_functions.insert("__syntax_spans", NativeFunction::new("__syntax_spans",
            NativeVisibility::Hidden, Rc::new(Interpreter::handle_syntax_spans)));
        native_functions.insert("__editor_source", NativeFunction::new("__editor_source",
            NativeVisibility::Hidden, Rc::new(Interpreter::handle_editor_source)));

        native_functions.insert("__semantic_source", NativeFunction::new("__semantic_source",
            NativeVisibility::Hidden, Rc::new(Interpreter::handle_semantic_source)));

        native_functions.insert("__highlight_source", NativeFunction::new("__highlight_source",
            NativeVisibility::Hidden, Rc::new(Interpreter::handle_highlight_source)));

        let special_vars: SpecialVars = HashMap::from([
                (
                    "$SHELLY_STD_FOUND",
                    Rc::new(|interpreter: &Interpreter|
                        {
                            Ok(interpreter.scope().prelude_source.as_ref()
                                .map_or(Value::None, |source| Value::from_string(source.clone())))
                        }) as ReadFunction
                ),
                (
                    "$last_cmd_time",
                    Rc::new(|interpreter: &Interpreter|
                        {
                            let seconds = interpreter.last_cmd_time.as_secs();
                            let minutes = (seconds / 60) % 60;
                            let hours = (seconds / 3600) % 24;
                            let text = if seconds >= 86400
                                {
                                    format!("{}:{:02}:{:02}:{:02}", seconds / 86400,
                                            hours, minutes, seconds % 60)
                                }
                                else if seconds >= 3600
                                {
                                    format!("{}:{:02}:{:02}", hours, minutes, seconds % 60)
                                }
                                else
                                {
                                    format!("{}:{:02}", seconds / 60, seconds % 60)
                                };
                            Ok(Value::from_string(text))
                        }) as ReadFunction
                ),
                (
                    "$pid",
                    Rc::new(|_interpreter: &Interpreter|
                        {
                            Ok(Value::Integer(i64::from(id())))
                        }) as ReadFunction
                ),
                (
                    "$pwd",
                    Rc::new(|_interpreter: &Interpreter|
                        {
                            let cwd = current_dir().unwrap_or_else(|_| ".".into());
                            Ok(Value::from_string(cwd.display().to_string()))
                        }) as ReadFunction
                ),
                (
                    "$HOSTNAME",
                    Rc::new(|_interpreter: &Interpreter|
                        {
                            let name = get_hostname()
                                .map(|name| name.to_string_lossy().into_owned())
                                .unwrap_or_else(|_| "unknown".to_string());

                            Ok(Value::from_string(name))
                        }) as ReadFunction
                )
            ]);

        Self
            {
                scopes: HashMap::from([
                        (MAIN_SCOPE.to_string(), Scope::new(native_functions.values().cloned(),
                            MAIN_SCOPE.to_string(), TypeRegistry::new(), color_mode))
                    ]),
                current_scope: MAIN_SCOPE.to_string(),
                exceptions: Default::default(),
                protocol_context: Vec::new(),
                active_factories: Vec::new(),
                loading_modules: HashSet::new(),
                module_sources: HashMap::new(),
                pending_modules: HashMap::new(),
                prelude: None,
                prelude_loading: false,
                prelude_generation: 0,
                type_scopes: HashMap::new(),
                special_vars,
                native_functions,
                interactive: false,
                reactor: None,
                widget_environment: None,
                prompt_widgets: None,
                pipeline_inputs: Vec::new(),
                captured_stdout: None,
                redirections: Vec::new(),
                last_cmd_time: Duration::ZERO,
                color_mode,
                startup_input: Vec::new(),
                last_result: None,
                exit_code: 0,
                halted: false
            }
    }

    /**
     * Load login profiles, then the prelude, then interactive initialization and the banner.
     */
    fn initialize_startup(
        &mut self,
        startup: Startup,
        interactive: Interactive,
        color_mode: TtyColorMode,
        tab_width: usize,
        rc_file: RcFile,
        script_args: Vec<String>,
    )
    {
        let graphics = if    matches!(interactive, Interactive::Yes)
                      && color_mode != TtyColorMode::TtyMonochrome
            {
                let capabilities = detect_capabilities();
                let banner = if capabilities.iterm2 { iterm2_banner(&capabilities) } else { None }
                    .or_else(|| capabilities.sixel.then(|| sixel_banner(&capabilities)
                        .unwrap_or_else(|| BANNER_SIXEL.to_string())));
                self.startup_input = capabilities.input;
                banner
            }
            else
            {
                None
            };
        let banner = if let Some(graphics) = graphics
            {
                graphics
            }
            else
            {
                match color_mode
                {
                    TtyColorMode::TtyTrueColor => BANNER_TRUECOLOR,
                    TtyColorMode::Tty256 => BANNER_256,
                    TtyColorMode::TtyBasic | TtyColorMode::TtyMonochrome => BANNER_MONO
                }.to_string()
            };

        let is_interactive = matches!(
            interactive,
            Interactive::Yes | Interactive::YesWithoutBanner
        );

        self.interactive = is_interactive;
        self.set_variable("$build_date", Value::from_string(env!("SHELLY_BUILD_DATE").to_string()));
        self.set_variable("$build_time", Value::from_string(env!("SHELLY_BUILD_TIME").to_string()));
        self.set_variable("$version", Value::from_string(env!("CARGO_PKG_VERSION").to_string()));
        self.set_variable("$shelly", current_exe()
            .map(|path| Value::from_executable_string(path.display().to_string()))
            .unwrap_or_else(|_| Value::from_string(".".to_string())));
        self.set_variable("$OS",  Value::from_string(OS.to_string()));
        self.set_variable("$os",  Value::from_string(OS.to_string()));

        self.set_variable("$args",
            Value::from_array(script_args.iter().map(|arg|
                {
                    Value::from_string(arg.clone())
                })
                .collect()));

        if is_interactive
        {
            self.set_variable("$banner", Value::from_string(banner.clone()));
            self.set_variable("$interactive", Value::Boolean(true));
        }
        else
        {
            self.set_variable("$interactive", Value::Boolean(false));
        }

        self.set_variable("$login", Value::Boolean(matches!(startup, Startup::Login)));
        if matches!(startup, Startup::Login)
        {
            self.load_startup_script(Path::new("/etc/shelly/profile.shy"), tab_width);

            if let Some(home) = home_dir()
            {
                self.load_startup_script(&home.join(".shelly_profile.shy"), tab_width);
            }
        }

        // Login profiles configure the standard-library path before its one-time load.
        if self.halted { return; }
        if let Err(error) = self.initialize_prelude()
        {
            eprintln!("Error loading standard prelude: {}", error);
            self.exit_code = 1;
            self.halted = true;
            return;
        }
        if self.halted { return; }

        if    is_interactive
           && rc_file != RcFile::None
        {
            let file = if let RcFile::Custom(file_path)= rc_file
                {
                    Some(file_path)
                }
                else
                {
                    if let Some(home) = home_dir()
                    {
                        Some(home.join(".shelly_init.shy"))
                    }
                    else
                    {
                        None
                    }
                };

            self.set_variable("$rc_path", Value::from_string(file.as_ref().map_or("".to_string(),
                |f| f.display().to_string())));

            if    let Some(file) = file
               && file.exists()
            {
                self.load_startup_script(&file, tab_width);
            }
            else
            {
                self.set_variable("$rc_path", Value::from_string("<not found>".to_string()));
            }
        }
        else
        {
            self.set_variable("$rc_path", Value::from_string("<unloaded>".to_string()));
        }

        if matches!(interactive, Interactive::Yes) && !self.halted
        {
            match self.evaluate_variable("$banner")
            {
                Ok(filtered_banner) => println!("{}", filtered_banner),

                Err(error) =>
                    {
                        self.set_variable("$banner", Value::from_string(banner.to_string()));
                        let banner = self.evaluate_variable("$banner").unwrap();

                        println!("{}", banner);
                        eprintln!("Error evaluating banner: {}", error)
                    }
            }
        }
    }

    /**
     * Execute an optional startup script, preserving its path in diagnostics.
     */
    fn load_startup_script(&mut self, path: &Path, tab_width: usize)
    {
        if self.halted
        {
            return;
        }

        let origin = self.eval_path_to(&path.to_string_lossy());
        let file = match File::open(path)
        {
            Ok(file) => file,
            Err(error) if error.kind() == ErrorKind::NotFound => return,
            Err(error) =>
                {
                    eprintln!("Error opening startup file {}: {}", origin, error);
                    return;
                }
        };

        let mut file_buffer = BufReader::new(file);
        let mut buffer = ReadBuffer::new(&origin, &mut file_buffer, Some(tab_width));

        if let Err(error) = self.execute_from_buffer(&mut buffer)
        {
            eprintln!("Error processing startup file {}: {}", origin, error);
        }
    }

    pub fn record_command_time(&mut self, elapsed: Duration)
    {
        self.last_cmd_time = elapsed;
    }

    pub fn capture_stdout<T>(&mut self,
                             run: impl FnOnce(&mut Self) -> InterpreterResult<T>)
                             -> (InterpreterResult<T>, Vec<u8>)
    {
        let previous = self.captured_stdout.replace(Vec::new());
        let result = run(self);

        let captured = replace(&mut self.captured_stdout,
                               previous).expect("Capture buffer must be installed.");

        (result, captured)
    }

    pub fn execute_from_buffer(&mut self, buffer: &mut dyn Buffer) -> InterpreterResult<()>
    {
        if self.halted { return Ok(()); }
        let origin = buffer.location().origin.to_string();
        let mut tokenizer = Tokenizer::new(buffer);
        let statements = parse_text(&mut tokenizer)?;
        self.execute_submission(statements, &origin)
    }

    pub fn execute_instructions(&mut self, instructions: &Vec<Instruction>)
    -> InterpreterResult<()>
    {
        if !crate::runtime::tasks::active()
        {
            if self.reactor.is_none()
            {
                self.reactor = Some(crate::runtime::tasks::Reactor::new().map_err(|error| {
                    InterpreterError::new(
                        Location::default(),
                        ErrorWhat::InvalidOperand(format!("Cannot create I/O reactor: {}", error)),
                    )
                })?);
            }
            let reactor = self.reactor.as_ref().unwrap().clone();
            return reactor
                .run(|| {
                    self.execute_instructions_with_arguments(instructions, &[], None, &[], false)
                })
                .map_err(|error| {
                    InterpreterError::new(
                        Location::default(),
                        ErrorWhat::InvalidOperand(format!(
                            "Cannot allocate VM continuation: {}",
                            error
                        )),
                    )
                })?;
        }
        self.execute_instructions_with_arguments(instructions, &[], None, &[], false)
    }

    fn execute_instructions_with_arguments(
        &mut self,
        instructions: &Vec<Instruction>,
        arguments: &[Value],
        receiver: Option<&ValueReference>,
        parameter_types: &[Option<TypeId>],
        function: bool,
    ) -> InterpreterResult<()>
    {
        let region = self.enter_failure_region();
        let initial_scope = self.scope().variables.current_scope();
        let initial_redirections = self.redirections.len();
        let result =
            self.execute_instructions_scoped(instructions, initial_scope, arguments, receiver,
                                             parameter_types);

        let cleanup = self.finish_redirections(initial_redirections);
        let result = self.combine_failure(result, cleanup, "Cleanup");
        let result = self.leave_failure_region(region, result, function);

        self.scope_mut().variables.reset_to_scope(initial_scope);
        if result.is_err() { self.last_result = None; }

        result
    }

    fn execute_instructions_scoped(
        &mut self,
        instructions: &Vec<Instruction>,
        initial_scope: usize,
        function_arguments: &[Value],
        receiver: Option<&ValueReference>,
        parameter_types: &[Option<TypeId>],
    ) -> InterpreterResult<()>
    {
        let mut stack: VecDeque<Value> = VecDeque::new();
        let mut references: Vec<ValueReference> = Vec::new();
        let mut iterations: Vec<Iteration> = Vec::new();
        let mut loops: Vec<LoopFrame> = Vec::new();
        let mut instruction_pointer: usize = 0;
        let mut location: Location = Location::default();
        if self.exceptions.transfer.is_none() && self.exceptions.cleanup.is_none()
        {
            self.exceptions.transfer_cleanup = None;
        }
        let mut quantum = 0usize;

        while    instruction_pointer < instructions.len()
              && !self.halted
        {
            let instruction = &instructions[instruction_pointer];

            if let Some(new_location) = instruction.location.clone()
            {
                location = new_location;
            }
            self.exceptions.last_location = location.clone();

            self.check_cleanup_budget(&location)?;
            if self.exceptions.cleanup.as_ref().is_none_or(|guard| guard.hook) &&
                self.exceptions.operation.as_ref().is_some_and(|(cancel, deadline)|
                cancel.iter().any(|flag| flag.load(std::sync::atomic::Ordering::Acquire)) ||
                    std::time::Instant::now() >= *deadline)
            { return Err(InterpreterError::new(location.clone(), ErrorWhat::OperationCancelled)); }
            quantum += 1;
            if quantum == 1024 && self.exceptions.cleanup.as_ref().is_none_or(|guard| guard.hook)
                && crate::runtime::tasks::active()
            {
                quantum = 0;
                crate::runtime::tasks::cooperate().map_err(|error|
                    InterpreterError::new(location.clone(), match error.kind.as_str()
                    {
                        "Interrupted" => ErrorWhat::Interrupted,
                        "CleanupAbort" => ErrorWhat::CleanupAbort,
                        _ => ErrorWhat::UnsupportedSuspension(error.message),
                    }))?;
            }
            if take_interrupt()
            {
                let what = if let Some(guard) = self.exceptions.cleanup.as_mut()
                    {
                        if guard.interrupted { ErrorWhat::CleanupAbort }
                        else { guard.interrupted = true; ErrorWhat::Interrupted }
                    }
                    else { ErrorWhat::Interrupted };
                return Err(InterpreterError::new(location.clone(), what));
            }

            match instruction.code
            {
                Code::Try(ref protected) =>
                    {
                        self.execute_try(&location, protected)?;
                        if let Some(transfer) = self.exceptions.transfer
                        {
                            if transfer ==
                                crate::language::interpreter::exceptions::Transfer::Return
                                { return Ok(()); }
                            if loops.is_empty()
                            {
                                if self.exceptions.protected.last() ==
                                    Some(&self.exceptions.frames.len()) { return Ok(()); }
                                return Err(InterpreterError::new(location.clone(),
                                    ErrorWhat::LoopControlError("Cannot transfer outside a loop".into())));
                            }
                            let frame = loops.last().unwrap();
                            self.leave_control_scopes(frame.failure_depth)?;
                            self.finish_redirections(frame.redirection_depth)?;
                            self.scope_mut().variables.reset_to_scope(frame.scope);
                            stack.truncate(frame.stack_depth);
                            iterations.truncate(frame.iteration_depth);
                            references.truncate(frame.reference_depth);
                            self.exceptions.transfer = None;
                            instruction_pointer = if transfer ==
                                crate::language::interpreter::exceptions::Transfer::Break
                                { frame.break_target } else { frame.continue_target };
                            continue;
                        }
                    },
                Code::Throw =>
                    {
                        let mut value = self.last_result.take().unwrap_or(Value::None);
                        self.activate_failure(&location, &mut value, true)?;
                        return Err(self.raise_failure(&location, value)?);
                    },
                Code::Pipeline(ref pipeline) => self.execute_pipeline(pipeline)?,
                Code::BeginRedirect =>
                    {
                        let Some(Value::Array(parts)) = &instruction.operand else
                        { unreachable!("Invalid redirection operand"); };
                        let [Value::Integer(stream), Value::Boolean(variable),
                             Value::Integer(pending)] = parts.as_slice()
                        else { unreachable!("Invalid redirection operand"); };
                        let index = stack.len().checked_sub(*pending as usize)
                            .ok_or_else(|| InterpreterError::new(
                                location.clone(),
                                ErrorWhat::StackUnderflow
                            ))?;
                        let target = stack.remove(index).ok_or_else(|| InterpreterError::new(
                            location.clone(),
                            ErrorWhat::StackUnderflow
                        ))?;
                        self.begin_redirection(&location, *stream, *variable, target)?;
                    },

                Code::EndRedirect =>
                    {
                        self.finish_redirections(self.redirections.len() - 1)?;
                    },

                Code::RedirectSource =>
                    {
                        let Some(Value::Boolean(command_word)) = &instruction.operand
                        else { unreachable!("Invalid redirection source operand"); };
                        let value = self.last_result.take().ok_or_else(|| InterpreterError::new(
                            location.clone(),
                            ErrorWhat::NoResult
                        ))?;
                        self.redirect_source(&location, value, *command_word)?;
                    },

                Code::Push =>
                    {
                        if let Some(operand) = &instruction.operand
                        {
                            Self::push(&mut stack, self.bind_executable(operand.clone()));
                        }
                        else
                        {
                            let message = "Missing operand for Push instruction.".to_string();

                            return Err(InterpreterError::new(
                                location.clone(),
                                ErrorWhat::InvalidOperand(message)
                            ));
                        }
                    },

                Code::ExitFunction =>
                    {
                        self.check_cleanup_return(&location)?;
                        self.exceptions.transfer = Some(
                            crate::language::interpreter::exceptions::Transfer::Return);
                        if !self.scope().in_function()
                        {
                            return Err(InterpreterError::new(
                                location.clone(),
                                ErrorWhat::ReturnOutsideFunction
                            ));
                        }

                        return Ok(());
                    },

                Code::TryExecute =>
                    {
                        let value = Self::pop(&location, &mut stack)?;
                        if value.is_callable() || value.is_external()
                        {
                            self.execute_value(&location, value, Vec::new())?;
                            instruction_pointer += 1;
                            continue;
                        }
                        let executable = value.as_text();

                        if self.can_execute(&executable)
                        {
                            let value =
                                self.bind_executable(Value::from_executable_string(executable));
                            self.execute_value(&location, value, Vec::new())?;
                        }
                        else
                        {
                            self.last_result = Some(Value::from_string(executable));
                        }
                    },

                Code::MakeClosure =>
                    {
                        let Some(Value::Array(parts)) = &instruction.operand
                            else { unreachable!(); };
                        let Value::String(_, Executable::Function(function)) = &parts[0]
                            else { unreachable!(); };
                        let Value::Array(names) = &parts[1] else { unreachable!(); };
                        let mut captures = ScopedVariables::empty();
                        for name in names.iter()
                        {
                            let Value::String(name, _) = name else { unreachable!(); };
                            if let Some(binding) = self.variable_binding(name)
                            {
                                let value = binding.borrow().value.clone();
                                self.consume_success(&location, &value)?;
                                self.exceptions.captures.insert(Rc::as_ptr(&binding) as usize);
                                captures.import(name.clone(), binding);
                            }
                        }
                        self.last_result = Some(Value::Closure(Rc::new(ClosureValue::new(
                            function.clone(), captures))));
                    },

                Code::MakeExecutable =>
                    {
                        let value = self.last_result.take().ok_or_else(|| InterpreterError::new(
                            location.clone(),
                            ErrorWhat::NoResult
                        ))?;
                        if matches!(value, Value::Type(_))
                        {
                            return Err(InterpreterError::new(
                                location.clone(),
                                ErrorWhat::InvalidOperand(
                                    "Cannot execute a type as a command".into()
                                )
                            ));
                        }
                        if matches!(value, Value::Enum(_) | Value::Struct(_) | Value::Terminal(_))
                        {
                            return Err(InterpreterError::new(
                                location.clone(),
                                ErrorWhat::InvalidOperand(
                                    format!(
                                        "Cannot execute {} as a command",
                                        if matches!(value, Value::Enum(_))
                                        {
                                            "an enum"
                                        }
                                        else if matches!(value, Value::Terminal(_))
                                        {
                                            "a terminal"
                                        }
                                        else
                                        {
                                            "a struct"
                                        }
                                    )
                                )
                            ));
                        }
                        self.last_result = Some(match value
                            {
                                value if value.is_callable() || value.is_external() =>
                                        value,
                                value =>
                                    self.bind_executable(
                                        Value::from_executable_string(value.as_text()))
                            });
                    },

                Code::ExecuteIfExecutable =>
                    {
                        match self.last_result.take()
                        {
                            Some(value) if value.is_executable() =>
                                {
                                    self.execute_value(&location, value, Vec::new())?;
                                },

                            Some(
                                value @ (Value::Array(_)
                                | Value::ArgumentExpansion(_)
                                | Value::HashMap(_)
                                | Value::Range(_)
                                | Value::Enum(_)
                                | Value::Struct(_)
                                | Value::Terminal(_)),
                            )
                                if matches!(instruction.operand, Some(Value::Boolean(true))) =>
                                {
                                    Self::command_name(&location, value)?;
                                },

                            Some(value) => self.last_result = Some(value),

                            None => return Err(InterpreterError::new(
                                location.clone(),
                                ErrorWhat::NoResult
                            ))
                        }
                    },

                Code::PublishSymbol =>
                    {
                        let value = Self::pop(&location, &mut stack)?;
                        self.publish_symbol(&location, &value)?;
                        self.last_result = Some(Value::None);
                    },

                Code::Execute | Code::PreparePipelineCall =>
                    {
                        let mut args = Vec::new();

                        if let Some(operand) = &instruction.operand
                        {
                            let value = operand.as_integer();

                            if value < 0
                            {
                                let message = "Negative argument count for Execute instruction.";

                                return Err(InterpreterError::new(
                                    location.clone(),
                                    ErrorWhat::InvalidOperand(message.to_string())
                                ));
                            }

                            for _ in 0..value
                            {
                                let arg = Self::pop(&location, &mut stack)?;

                                if let Value::ArgumentExpansion(expanded_args) = arg
                                {
                                    for expanded_arg in expanded_args.iter().rev()
                                    {
                                        args.push(expanded_arg.clone());
                                    }
                                }
                                else
                                {
                                    args.push(arg);
                                }
                            }
                        }
                        else
                        {
                            return Err(InterpreterError::new(
                                location.clone(),
                                ErrorWhat::InvalidOperand(
                                    "Missing operand for Execute instruction.".to_string()
                                )
                            ));
                        }

                        args.reverse();

                        let mut executable = Self::pop(&location, &mut stack)?;
                        let name = Self::command_name(&location, executable.clone())?;

                        // If the executable name starts with a $ eval as a variable first.
                        if name.starts_with('$')
                        {
                            if name.contains('/')
                            {
                                executable = Value::from_executable_string(
                                    self.interpolate_string(&location, &name)?,
                                );
                            }
                            else
                            {
                                executable = self.read_raw_variable(&name, &location)?;
                            }
                        }

                        if matches!(instruction.code, Code::PreparePipelineCall)
                        {
                            self.last_result = Some(Value::from_array(vec![
                                    executable, Value::from_array(args),
                                ]));
                        }
                        else { self.execute_value(&location, executable, args)?; }
                    }

                Code::NewAlias =>
                    {
                        let definition = match &instruction.operand
                            {
                                Some(Value::Array(values)) => match values.as_slice()
                                    {
                                        [Value::String(alias, _), Value::Integer(count)]
                                            if !alias.is_empty() && *count >= 0 =>
                                                Some((alias, *count)),
                                        _ => None
                                    },
                                _ => None
                            };

                        let Some((alias, count)) = definition else
                        {
                            return Err(InterpreterError::new(
                                location.clone(),
                                ErrorWhat::InvalidOperand(
                                    "NewAlias requires a nonempty alias name and a \
                                    nonnegative argument count."
                                    .to_string()
                                )
                            ));
                        };

                        let mut arguments = Vec::new();
                        for _ in 0..count
                        {
                            arguments.push(Self::pop_as_text(&location, &mut stack)?);
                        }
                        arguments.reverse();
                        let target = Self::pop_as_text(&location, &mut stack)?;

                        let scope = self.current_scope.clone();
                        self.scope_mut().aliases.insert(alias.clone(),
                            Rc::new(Alias { scope, name: target, arguments }));
                    },

                Code::BindParameter | Code::BindRestParameter =>
                    {
                        let invalid = |message| InterpreterError::new(
                            location.clone(),
                            ErrorWhat::ArgumentMismatch(message)
                        );
                        let Some(Value::Array(parts)) = &instruction.operand else
                        { return Err(invalid("Invalid parameter binding operand".to_string())); };
                        let [
                            Value::String(name, _),
                            constraint,
                            Value::Integer(index),
                            defaults @ ..,
                        ] = parts.as_slice()
                        else
                        {
                            return Err(invalid("Invalid parameter binding operand".to_string()));
                        };
                        let mut value = if matches!(instruction.code, Code::BindRestParameter)
                            {
                                Value::from_array(
                                    function_arguments.get(*index as usize..).unwrap_or(&[])
                                        .to_vec())
                            }
                            else
                            {
                                function_arguments.get(*index as usize).or_else(|| defaults.first())
                                    .cloned()
                                    .ok_or_else(
                                        || invalid(format!("Missing argument for parameter '{}'",
                                                           name)))?
                            };
                        let type_id = match parameter_types.get(*index as usize)
                        {
                            Some(id) => *id,
                            None => match constraint
                            {
                                Value::Integer(id) => Some(TypeId(*id as usize)),
                                Value::None => None,
                                _ => return Err(invalid("Invalid parameter constraint".to_string()))
                            },
                        };
                        self.prepare_failure_value(&location, &mut value, type_id)?;
                        if let Some(id) = type_id
                        {
                            value = self.scope().types.coerce(id, value).map_err(|message|
                                invalid(format!("Type error for parameter '{}': {}", name,
                                                message)))?;
                        }
                        let reference = if *index == 0 && name == "$self"
                            {
                                receiver.cloned().map(|mut reference|
                                    {
                                        if let Some(id) = type_id
                                        {
                                            reference.constraints
                                                .push((reference.indexes.len(), id));
                                        }
                                        reference
                                    })
                            }
                            else { None };
                        self.scope_mut().variables
                            .create(
                                name.clone(),
                                ScopedValue
                                    {
                                        value,
                                        type_id,
                                        exported: ValueVisibility::Private,
                                        reference,
                                    },
                            )
                            .map_err(invalid)?;
                        self.register_failure_binding(name);
                    },

                Code::NewVariable | Code::BindIteration =>
                    {
                        let (variable_name, type_id) = match &instruction.operand
                            {
                                Some(Value::String(name, _)) => (name.clone(), None),
                                Some(Value::Array(parts)) if matches!(instruction.code,
                                    Code::NewVariable) =>
                                    {
                                        let [Value::String(name, _),
                                             Value::Integer(id)] = parts.as_slice() else
                                        { return Err(InterpreterError::new(
                                            location.clone(),
                                            ErrorWhat::InvalidOperand(
                                                "Invalid typed variable operand".to_string()
                                            )
                                        )); };
                                        (name.clone(), Some(TypeId(*id as usize)))
                                    },
                                _ => return Err(InterpreterError::new(
                                    location.clone(),
                                    ErrorWhat::InvalidOperand(
                                        "Missing or invalid operand for NewVariable \
                                        instruction."
                                        .to_string()
                                    )
                                ))
                            };

                        let type_id = type_id.or_else(||
                            matches!(instruction.code, Code::NewVariable)
                                .then(|| stack.back().and_then(Value::collection_type)).flatten());
                        let mut value = if matches!(instruction.code, Code::BindIteration)
                            { Self::pop(&location, &mut stack)? }
                            else if variable_name == "$HOME"
                            {
                                // Keep the old anchor until SetVariable resolves a shortened
                                // initializer, including `let $HOME = $HOME` in a nested scope.
                                self.read_raw_variable("$HOME", &location).unwrap_or(Value::None)
                            }
                            else { Value::None };
                        if variable_name == "$HOME" && let Value::String(path, _) = &mut value
                        {
                            *path = self.eval_path_from(path);
                        }
                        if let Err(error) = self.scope_mut().variables.create(variable_name.clone(),
                            ScopedValue
                                {
                                    value,
                                    type_id,
                                    exported: ValueVisibility::Private,
                                    reference: None,
                                })
                        {
                            return Err(InterpreterError::new(
                                location.clone(),
                                ErrorWhat::InvalidOperand(
                                    format!("Failed to create variable: {}", error)
                                )
                            ));
                        }
                        self.register_failure_binding(&variable_name);
                    },

                Code::ValidateType =>
                    {
                        let Some(Value::Integer(id)) = instruction.operand
                        else
                        {
                            return Err(InterpreterError::new(
                                location.clone(),
                                ErrorWhat::InvalidOperand(
                                    "Invalid ValidateType operand".to_string()
                                )
                            ));
                        };
                        let value = stack.back_mut().ok_or_else(|| InterpreterError::new(
                            location.clone(),
                            ErrorWhat::InvalidOperand(
                                "Missing value for type validation".to_string()
                            )
                        ))?;
                        self.prepare_failure_value(&location, value, Some(TypeId(id as usize)))?;
                        *value = self.scope().types
                            .coerce(TypeId(id as usize), value.clone())
                            .map_err(|message| InterpreterError::new(
                                location.clone(),
                                ErrorWhat::InvalidOperand(
                                    format!("Type error: {}", message)
                                )
                            ))?;
                    },

                Code::SetVariable =>
                    {
                        let variable_name = match &instruction.operand
                            {
                                Some(Value::String(name, _)) => name.clone(),
                                _ => return Err(InterpreterError::new(
                                    location.clone(),
                                    ErrorWhat::InvalidOperand(
                                        "Missing or invalid operand for SetVariable \
                                        instruction."
                                        .to_string()
                                    )
                                ))
                            };

                        let mut value = Self::pop(&location, &mut stack)?;

                        let type_id = self.variable_binding(&variable_name)
                            .and_then(|variable| variable.borrow().type_id);
                        match value
                        {
                            Value::ArgumentExpansion(array) if type_id.is_none() =>
                            {
                                value = Value::Array(array);
                            }
                            _ => {}
                        }

                        if variable_name == "$HOME" && let Value::String(path, _) = &mut value
                        {
                            *path = self.eval_path_from(path);
                        }

                        self.write_variable(&location, &variable_name, value)?;
                    },

                Code::StartIteration =>
                    {
                        let value = Self::pop(&location, &mut stack)?;
                        self.consume_success(&location, &value)?;
                        let snapshot = instruction.operand.as_ref().unwrap_or(&Value::None);
                        let iteration = Iteration::new(self, &location, value, snapshot)?;
                        iterations.push(iteration);
                    },

                Code::NextIteration =>
                    {
                        let iteration = iterations.last_mut().ok_or_else(|| InterpreterError::new(
                            location.clone(),
                            ErrorWhat::InvalidOperand("No active iteration".to_string())
                        ))?;
                        if let Some(value) = iteration.next(self, &location)?
                        {
                            Self::push(&mut stack, value);
                            self.last_result = Some(Value::Boolean(true));
                        }
                        else
                        {
                            self.last_result = Some(Value::Boolean(false));
                        }
                    },

                Code::EndIteration =>
                    {
                        iterations.pop().ok_or_else(|| InterpreterError::new(
                            location.clone(),
                            ErrorWhat::InvalidOperand("No active iteration".to_string())
                        ))?;
                        self.last_result = None;
                    },

                Code::MakeRange =>
                    {
                        let flags = match instruction.operand
                            {
                                Some(Value::Integer(flags)) if (0..=7).contains(&flags)
                                    && (flags & 4 == 0 || flags & 2 != 0) => flags,
                                _ => return Err(InterpreterError::new(
                                    location.clone(),
                                    ErrorWhat::InvalidOperand(
                                        "Invalid MakeRange flags.".to_string()
                                    )
                                ))
                            };
                        let end = if flags & 2 != 0
                        {
                            Some(Self::range_bound(
                                &location,
                                Self::pop(&location, &mut stack)?,
                            )?)
                        }
                        else
                        {
                            None
                        };
                        let start = if flags & 1 != 0
                        {
                            Some(Self::range_bound(
                                &location,
                                Self::pop(&location, &mut stack)?,
                            )?)
                        }
                        else
                        {
                            None
                        };
                        Self::push(
                            &mut stack,
                            Value::Range(Range
                                {
                                    start,
                                    end,
                                    inclusive: flags & 4 != 0,
                                }),
                        );
                    },

                Code::MakeHashMap =>
                    {
                        let count = match instruction.operand
                            {
                                Some(Value::Integer(count)) if count >= 0 => count as usize,
                                _ => return Err(InterpreterError::new(
                                    location.clone(),
                                    ErrorWhat::InvalidOperand(
                                        "Invalid MakeHashMap count.".to_string()
                                    )
                                ))
                            };
                        let mut pairs = Vec::new();
                        for _ in 0..count
                        {
                            let value = Self::pop(&location, &mut stack)?;
                            let key = Self::pop(&location, &mut stack)?;
                            self.consume_success(&location, &value)?;
                            self.consume_success(&location, &key)?;
                            pairs.push((MapKey::from_value(&key), value));
                        }
                        // Restore source order so the last duplicate key wins.
                        Self::push(
                            &mut stack,
                            Value::from_hash_map(pairs.into_iter().rev().collect()),
                        );
                    },

                Code::UnpackArray =>
                    {
                        let Some(Value::Integer(count)) = instruction.operand else
                        {
                            return Err(InterpreterError::new(
                                location.clone(),
                                ErrorWhat::InvalidOperand(
                                    "Missing array destructuring length".to_string()
                                )
                            ));
                        };
                        let value = Self::pop(&location, &mut stack)?;
                        let values = match value.underlying()
                            {
                                Value::Array(values) | Value::ArgumentExpansion(values) => values,
                                _ => return Err(InterpreterError::new(
                                    location.clone(),
                                    ErrorWhat::ArrayError(
                                        "Array destructuring requires an array".to_string()
                                    )
                                ))
                            };
                        if usize::try_from(count).ok() != Some(values.len())
                        {
                            return Err(InterpreterError::new(
                                location.clone(),
                                ErrorWhat::ArrayError(
                                    format!(
                                        "Array destructuring expected {} elements, got {}",
                                        count,
                                        values.len()
                                    )
                                )
                            ));
                        }
                        for value in values.iter() { Self::push(&mut stack, value.clone()); }
                    },

                Code::MakeArray =>
                    {
                        let count = match instruction.operand
                            {
                                Some(Value::Integer(count)) if count >= 0 => count as usize,
                                _ => return Err(InterpreterError::new(
                                    location.clone(),
                                    ErrorWhat::InvalidOperand(
                                        "Invalid MakeArray count.".to_string()
                                    )
                                ))
                            };
                        let mut elements = Vec::new();
                        for _ in 0..count { elements.push(Self::pop(&location, &mut stack)?); }
                        elements.reverse();
                        let mut array = Vec::new();
                        for element in elements
                        {
                            self.consume_success(&location, &element)?;
                            match element
                            {
                                Value::ArgumentExpansion(values) =>
                                    array.extend(Rc::unwrap_or_clone(values)),
                                value => array.push(value)
                            }
                        }
                        Self::push(&mut stack, Value::from_array(array));
                    },

                Code::MakeStruct =>
                    {
                        let invalid = |message| InterpreterError::new(
                            location.clone(),
                            ErrorWhat::InvalidOperand(message)
                        );
                        let Some(Value::Array(parts)) = &instruction.operand else
                        { return Err(invalid("Invalid MakeStruct operand".to_string())); };
                        let [Value::Integer(id), Value::Array(indexes), snapshot,
                             Value::Boolean(raw)] = parts.as_slice() else
                        { return Err(invalid("Invalid MakeStruct operand".into())); };
                        let id = TypeId(*id as usize);
                        let definition = self.scope().types.get(id);
                        let fields = match &definition.kind
                            { TypeKind::Struct(fields) => fields.as_slice(), _ => &[] };
                        if !*raw && self.active_factories.contains(&id)
                        { return Err(invalid(
                            "Recursive constructor call; create raw storage inside ::new".into()));
                        }
                        let raw = *raw;
                        if raw && matches!(definition.kind, TypeKind::Enum(_))
                        { return Err(invalid("Use an explicit enum variant inside ::new".into())); }
                        let signature = if !raw
                            { self.scope().types.constructors.get(&id).cloned() }
                            else { None };
                        if    signature.is_some()
                           && !self.scope().types.constructor_permissions.contains(&id)
                        { return Err(invalid("Constructor is private to its module".into())); }
                        let mut values = signature.as_ref().map_or_else(
                            || vec![Value::None; fields.len()], |signature| signature.defaults());
                        for index in indexes.iter().rev()
                        {
                            let Value::Integer(index) = index else { unreachable!(); };
                            values[*index as usize] = Self::pop(&location, &mut stack)?;
                        }
                        let mut value;
                        if let Some(signature) = signature
                        {
                            let key = method_key(id, "new");
                            let function = match snapshot
                                {
                                    Value::HashMap(methods) => methods
                                        .get(&MapKey::String(key.clone())).and_then(|value|
                                            if let Value::String(_, Executable::Function(f)) = value
                                            { Some(f.clone()) } else { None }),
                                    _ => None,
                                }.or_else(|| self.scope().lexical_function(&key))
                                .or_else(|| self.module_method(id, &key))
                                .ok_or_else(|| invalid(
                                    "Constructor factory is not available".into()))?;
                            let args = signature.arguments(values).map_err(&invalid)?;
                            let previous = self.last_result.take();
                            let result = self.execute_function(&location,
                                &format!("{}::new", definition.name), &function, &args);
                            value = self.last_result.take().unwrap_or(Value::None);
                            self.last_result = previous;
                            result?;
                        }
                        else
                        {
                            for value in &values { self.consume_success(&location, value)?; }
                            value = Value::Struct(Rc::new(StructValue
                                {
                                    definition: definition.clone(), fields: values,
                                    constructing: raw, failure: None,
                                }));
                            if !raw { seal_constructed(&mut value); }
                            value = self.scope().types.coerce(id, value)
                                .map_err(|message| invalid(format!("Type error: {}", message)))?;
                        }
                        self.activate_failure(&location, &mut value, false)?;
                        Self::push(&mut stack, value);
                    },

                Code::ReferenceVariable =>
                    {
                        let Some(Value::String(name, _)) = &instruction.operand
                        else { unreachable!("ReferenceVariable requires a variable name"); };
                        let reference = if let Some(reference) =
                            self.variable_reference(name)
                            { reference }
                            else
                            { ValueReference::temporary(self.read_variable(name, &location)?) };
                        self.read_reference(&location, &reference)?;
                        references.push(reference);
                    },

                Code::ReferenceValue =>
                    {
                        let value = Self::pop(&location, &mut stack)?;
                        references.push(ValueReference::temporary(value));
                    },

                Code::ReferenceIndex =>
                    {
                        let index = Self::pop(&location, &mut stack)?;
                        let mut reference = references.pop().expect("Missing receiver reference");
                        Self::get_element(&location,
                            &self.read_reference(&location, &reference)?, &index)?;
                        reference.indexes.push(index);
                        reference.fields.push(Value::Boolean(false));
                        references.push(reference);
                    },

                Code::ReferenceGroup =>
                    {
                        let mut reference = references.pop().expect("Missing receiver reference");
                        let value = self.read_reference(&location, &reference)?;
                        // Field mode executes a callable data member. An intermediate method
                        // already ran and produced a temporary reference with an empty path.
                        let execute = matches!(instruction.operand, Some(Value::Boolean(true)))
                            || matches!(instruction.operand, Some(Value::Integer(1)))
                                && !reference.indexes.is_empty();
                        if    execute
                           && value.is_executable()
                        {
                            let previous = self.last_result.take();
                            let result = self.execute_value(&location, value, Vec::new());
                            let value = self.last_result.take().unwrap_or(Value::None);
                            self.last_result = previous;
                            result?;
                            reference = ValueReference::temporary(value);
                        }
                        references.push(reference);
                    },

                Code::GetField | Code::BindField | Code::ReferenceField =>
                    {
                        let mut reference = references.pop().expect("Missing receiver reference");
                        let value = self.eval_value_paths_to(
                            self.read_reference(&location, &reference)?);
                        let Some(Value::Array(operands)) = &instruction.operand else
                        {
                            return Err(InterpreterError::new(
                                location.clone(),
                                ErrorWhat::InvalidOperand(
                                    "Missing field operand".to_string()
                                )
                            ));
                        };
                        let field = &operands[0];
                        let field = self.resolve_field_name(&location, field)?;
                        let (result, resolved) = self.resolve_field(
                            &location, reference, value, &field, &operands[1],
                            matches!(instruction.code, Code::BindField))?;
                        reference = resolved;
                        if matches!(instruction.code, Code::ReferenceField)
                        { references.push(reference); }
                        else { Self::push(&mut stack, result); }
                    },

                Code::GetElement =>
                    {
                        let index = Self::pop(&location, &mut stack)?;
                        let collection = Self::pop(&location, &mut stack)?;
                        let value = Self::get_element(&location, &collection, &index)?;
                        Self::push(&mut stack, value);
                    },

                Code::ResolveFieldName =>
                    {
                        let field = instruction.operand.as_ref().expect("Missing field name");
                        let field = self.resolve_field_name(&location, field)?;
                        Self::push(&mut stack, field);
                    },

                Code::SetElement =>
                    {
                        let (name, fields) = match &instruction.operand
                            {
                                Some(Value::Array(parts)) => match parts.as_slice()
                                    {
                                        [Value::String(name, _),
                                         Value::Array(fields)] if !fields.is_empty() =>
                                            (name, fields),
                                        _ => return Err(InterpreterError::new(
                                            location.clone(),
                                            ErrorWhat::InvalidOperand(
                                                "Invalid SetElement operand.".to_string()
                                            )
                                        ))
                                    },
                                _ => return Err(InterpreterError::new(
                                    location.clone(),
                                    ErrorWhat::InvalidOperand(
                                        "Missing SetElement operand.".to_string()
                                    )
                                ))
                            };
                        let value = Self::pop(&location, &mut stack)?;
                        let mut indexes = Vec::new();
                        for _ in 0..fields.len()
                        {
                            indexes.push(Self::pop(&location, &mut stack)?);
                        }
                        let value = match value
                            {
                                Value::ArgumentExpansion(values) if !matches!(fields.last(),
                                    Some(Value::Boolean(true))) => Value::Array(values),
                                value => value
                            };
                        indexes.reverse();
                        let mut updated = self.read_raw_variable(name, &location)?;
                        self.set_element(&location, &mut updated, &indexes, fields, value)?;
                        let updated = self.scope().types
                            .coerce_value(updated)
                            .map_err(|message| InterpreterError::new(
                                location.clone(),
                                ErrorWhat::InvalidOperand(
                                    format!("Type error: {}", message)
                                )
                            ))?;
                        self.write_variable(&location, name, updated)?;
                    },

                Code::GetVariable =>
                    {
                        let variable_name = match &instruction.operand
                            {
                                Some(Value::String(name, _)) => name.clone(),
                                _ => return Err(InterpreterError::new(
                                    location.clone(),
                                    ErrorWhat::InvalidOperand(
                                        "Missing or invalid operand for GetVariable \
                                        instruction."
                                        .to_string()
                                    )
                                ))
                            };

                        let value = self.read_variable(&variable_name, &location)?;
                        Self::push(&mut stack, value);
                    },

                Code::PushResult =>
                    {
                        let result = self.last_result.take();

                        if let Some(result) = result
                        {
                            Self::push(&mut stack, result);
                        }
                        else
                        {
                            // Something was really wrong in the bytecode; we expected a result but
                            // found none. Error out hard.
                            self.halted = true;

                            return Err(InterpreterError::new(
                                location.clone(), ErrorWhat::NoResult));
                        }
                    },

                Code::PopResult =>
                    {
                        // Attempt to pop a value from the stack and then store it as the last
                        // result.
                        let result = Self::pop(&location, &mut stack)?;
                        self.last_result = Some(result);
                    },

                Code::CheckResult =>
                    {
                        // Extract and clear the last result.
                        let result = self.last_result.take();
                        if let Some(value) = &result { self.consume_success(&location, value)?; }

                        // If there was an actual value stored in the last result, check it. If the
                        // value was an execution result, check its return code. If it's bad, halt
                        // script execution.
                        if    let Some(result) = result
                           && let Value::ExecResult(result) = result
                        {
                            match result
                            {
                                ExecResult::Value(0) => {},

                                ExecResult::Value(code) =>
                                    {
                                        return Err(InterpreterError::new(
                                            location.clone(),
                                            ErrorWhat::ExecutableBadReturn(code)
                                        ));
                                    },

                                ExecResult::Signaled =>
                                    {
                                        return Err(InterpreterError::new(
                                            location.clone(),
                                            ErrorWhat::ExecutableSignaled
                                        ));
                                    }
                            }
                        }
                    },

                Code::ExportVariable =>
                    {
                        let variable_name = match &instruction.operand
                            {
                                Some(Value::String(name, _)) => name.clone(),
                                _ => return Err(InterpreterError::new(
                                    location.clone(),
                                    ErrorWhat::InvalidOperand(
                                        "Missing or invalid operand for ExportVariable \
                                        instruction."
                                        .to_string()
                                    )
                                ))
                            };

                        if let Some(mut value) = self.scope_mut().variables.get_mut(&variable_name)
                        {
                            value.exported = ValueVisibility::Exported;
                        }
                        else
                        {
                            return Err(InterpreterError::new(
                                location.clone(),
                                ErrorWhat::InvalidOperand(
                                    "Variable not found for ExportVariable \
                                    instruction.".to_string()
                                )
                            ));
                        }
                    },

                Code::GlobFiles =>
                    {
                        let pattern = Self::pop_as_text(&location, &mut stack)?;
                        let expand_tilde = matches!(instruction.operand,
                                                    Some(Value::Boolean(true)));
                        stack.push_back(self.handle_file_glob(&pattern, expand_tilde)
                            .map_err(|mut error|
                                {
                                    error.location = location.clone();
                                    error
                                })?);
                    },

                Code::ExpandPath =>
                    {
                        let value = Self::pop(&location, &mut stack)?;
                        let path = self.eval_path_from(&value.as_text());
                        let expanded = match value
                            {
                                Value::String(_, executable) => Value::String(path, executable),
                                _ => Value::from_string(path)
                            };
                        Self::push(&mut stack, expanded);
                    },

                Code::ExpandArray =>
                    {
                        let mut value = Self::pop(&location, &mut stack)?.underlying().clone();

                        match value
                        {
                            Value::Array(array) => value = Value::ArgumentExpansion(array),

                            Value::Range(range) =>
                                {
                                    let error = |message: &str| InterpreterError::new(
                                        location.clone(),
                                        ErrorWhat::RangeError(message.to_string())
                                    );
                                    let length = range.len()
                                        .ok_or_else(
                                            || error("Cannot expand a range with omitted bounds"))?;
                                    let length = usize::try_from(length)
                                        .map_err(|_| error("Range is too large to expand"))?;
                                    let mut values = Vec::new();
                                    values
                                        .try_reserve_exact(length)
                                        .map_err(|_| error("Range is too large to expand"))?;
                                    values.extend(range.iter().unwrap().map(Value::Integer));
                                    value = Value::from_argument_expansion(values);
                                },

                            Value::ArgumentExpansion(_) => {},

                            _ =>
                                {
                                    value = Value::from_argument_expansion(vec![value]);
                                }
                        }

                        Self::push(&mut stack, value);
                    },

                Code::InterpolateString | Code::InterpolateGlob =>
                    {
                        let escaped_dollars = match &instruction.operand
                            {
                                Some(Value::Array(offsets)) => offsets.iter()
                                    .map(|offset| offset.as_integer() as usize).collect(),
                                _ => Vec::new()
                            };
                        self.handle_string_interpolation(&location, &mut stack, &escaped_dollars,
                            matches!(instruction.code, Code::InterpolateGlob))?;
                    },

                Code::EnterScope =>
                    {
                        self.scope_mut().variables.push_scope();
                        self.enter_failure_region();
                    },

                Code::ExitScope =>
                    {
                        if self.scope().variables.current_scope() == initial_scope
                        {
                            return Err(InterpreterError::new(
                                location.clone(),
                                ErrorWhat::InitialScopePopAttempt
                            ));
                        }

                        let region = *self.exceptions.regions.last().unwrap();
                        let expression = matches!(instruction.operand, Some(Value::Boolean(true)));
                        self.leave_scope_region(region, Ok(()), false, expression)?;
                        self.scope_mut().variables.pop_scope();
                    },

                Code::EnterLoop =>
                    {
                        let Some(Value::Array(targets)) = &instruction.operand else
                        {
                            return Err(InterpreterError::new(
                                location.clone(),
                                ErrorWhat::InvalidOperand(
                                    "Expected two linked loop targets".to_string()
                                )
                            ));
                        };
                        if targets.len() != 2
                        {
                            return Err(InterpreterError::new(
                                location.clone(),
                                ErrorWhat::InvalidOperand(
                                    "Expected two linked loop targets".to_string()
                                )
                            ));
                        }
                        loops.push(LoopFrame
                            {
                                continue_target: Self::jump_target(
                                    instructions,
                                    targets.first(),
                                    &location,
                                )?,
                                break_target: Self::jump_target(instructions, targets.get(1),
                                                                &location)?,
                                scope: self.scope().variables.current_scope(),
                                stack_depth: stack.len(),
                                iteration_depth: iterations.len(),
                                reference_depth: references.len(),
                                redirection_depth: self.redirections.len(),
                                failure_depth: self.exceptions.regions.len(),
                            });
                    },

                Code::ExitLoop =>
                    {
                        loops.pop().ok_or_else(|| InterpreterError::new(
                            location.clone(),
                            ErrorWhat::InvalidOperand(
                                "No active loop to exit".to_string()
                            )
                        ))?;
                    },

                Code::Break | Code::Continue =>
                    {
                        if loops.is_empty() && self.exceptions.protected.last() ==
                            Some(&self.exceptions.frames.len())
                        {
                            self.check_cleanup_return(&location)?;
                            self.exceptions.transfer = Some(if matches!(instruction.code,
                                Code::Break)
                                { crate::language::interpreter::exceptions::Transfer::Break }
                                else {
                                    crate::language::interpreter::exceptions::Transfer::Continue });
                            return Ok(());
                        }
                        let frame = loops.last().ok_or_else(|| InterpreterError::new(
                            location.clone(),
                            ErrorWhat::LoopControlError(
                                format!(
                                    "Cannot {} outside a loop",
                                    if matches!(instruction.code, Code::Break)
                                    {
                                        "break"
                                    }
                                    else
                                    {
                                        "continue"
                                    }
                                )
                            )
                        ))?;
                        // A transfer may abandon nested blocks and partially evaluated
                        // expressions. Restore the state saved before the iteration body.
                        self.leave_control_scopes(frame.failure_depth)?;
                        self.finish_redirections(frame.redirection_depth)?;
                        self.scope_mut().variables.reset_to_scope(frame.scope);
                        stack.truncate(frame.stack_depth);
                        iterations.truncate(frame.iteration_depth);
                        references.truncate(frame.reference_depth);
                        self.last_result = None;
                        instruction_pointer = if matches!(instruction.code, Code::Break)
                            { frame.break_target } else { frame.continue_target };
                        continue;
                    },

                Code::JumpTarget => {},

                Code::Jump | Code::JumpIfFalse | Code::JumpIfTrue =>
                    {
                        let target =
                            Self::jump_target(instructions, instruction.operand.as_ref(),
                                              &location)?;
                        let jump = if matches!(instruction.code, Code::Jump)
                            {
                                true
                            }
                            else
                            {
                                let Some(Value::Boolean(condition)) = &self.last_result else
                                {
                                    return Err(InterpreterError::new(
                                        location.clone(),
                                        ErrorWhat::InvalidOperand(
                                            "Expected a boolean jump condition".to_string()
                                        )
                                    ));
                                };
                                *condition == matches!(instruction.code, Code::JumpIfTrue)
                            };
                        if jump
                        {
                            instruction_pointer = target;
                            continue;
                        }
                    },

                Code::ConvertType =>
                    {
                        let Some(Value::Array(operands)) = &instruction.operand else
                        {
                            return Err(InterpreterError::new(
                                location.clone(),
                                ErrorWhat::InvalidOperand(
                                    "Missing conversion target".to_string()
                                )
                            ));
                        };
                        let Value::Integer(id) = operands[0] else { unreachable!(); };
                        let value = self.last_result.take().ok_or_else(|| InterpreterError::new(
                            location.clone(),
                            ErrorWhat::NoResult
                        ))?;
                        self.convert_value(&location, TypeId(id as usize), value,
                                           &operands[1], &operands[2])?;
                    },

                Code::ToBoolean | Code::BooleanNot =>
                    {
                        if let Some(value) = self.last_result.clone()
                        { self.consume_success(&location, &value)?; }
                        let value = self.last_result.take().ok_or_else(|| InterpreterError::new(
                            location.clone(),
                            ErrorWhat::NoResult
                        ))?;
                        let value = value.as_bool();
                        self.last_result = Some(Value::Boolean(
                            if matches!(instruction.code, Code::BooleanNot)
                            {
                                !value
                            }
                            else
                            {
                                value
                            },
                        ));
                    },

                Code::MathNegate =>
                    {
                        let error = |message: &str| InterpreterError::new(
                            location.clone(),
                            ErrorWhat::ArithmeticError(message.to_string())
                        );
                        let mut value = self.last_result.take().unwrap_or(Value::None);
                        self.consume_success(&location, &value)?;
                        let mut named = Vec::new();
                        while let Value::Named(item) = value
                        {
                            named.push(item.definition.id);
                            value = item.value.clone();
                        }
                        if let Some(message) = value.arithmetic_error()
                        { return Err(error(message)); }
                        value = match value
                            {
                                Value::Integer(number) => Value::Integer(number.checked_neg()
                                    .ok_or_else(|| error("Integer overflow"))?),
                                Value::Float(number, _) if number.is_finite() =>
                                    Value::Float(-number, None),
                                _ => return Err(error("Non-finite floating-point operand")),
                            };
                        for id in named.into_iter().rev()
                        {
                            value = self.scope().types.coerce(id, value)
                                .map_err(|message| error(&message))?;
                        }
                        self.last_result = Some(value);
                    },

                Code::Discard =>
                    {
                        let value = Self::pop(&location, &mut stack)?;
                        self.consume_success(&location, &value)?;
                    },

                Code::MatchFail => return Err(InterpreterError::new(
                    location.clone(),
                    ErrorWhat::MatchError
                )),

                Code::MatchPattern =>
                    {
                        let pattern = Self::pop(&location, &mut stack)?;
                        let subject = stack.back().ok_or_else(|| InterpreterError::new(
                            location.clone(),
                            ErrorWhat::StackUnderflow
                        ))?;
                        let matched = match (&pattern, subject)
                            {
                                (Value::Range(range), Value::Integer(value)) =>
                                    range.contains(*value),
                                (Value::Range(_), _) => false,
                                _ => subject.equals(&pattern),
                            };
                        if matched { stack.pop_back(); }
                        self.last_result = Some(Value::Boolean(matched));
                    },

                Code::CompareEqual | Code::CompareNotEqual =>
                    {
                        let rhs = Self::pop(&location, &mut stack)?;
                        let lhs = Self::pop(&location, &mut stack)?;
                        let equal = lhs.equals(&rhs);
                        Self::push(
                            &mut stack,
                            Value::Boolean(
                                if matches!(instruction.code, Code::CompareEqual)
                                {
                                    equal
                                }
                                else
                                {
                                    !equal
                                },
                            ),
                        );
                    },

                Code::CompareLess | Code::CompareLessEqual
                | Code::CompareGreater | Code::CompareGreaterEqual =>
                    {
                        let rhs = Self::pop(&location, &mut stack)?;
                        let lhs = Self::pop(&location, &mut stack)?;
                        self.consume_success(&location, &lhs)?;
                        self.consume_success(&location, &rhs)?;
                        let order = lhs.numeric_order(&rhs).map_err(|message| InterpreterError::new(
                            location.clone(),
                            ErrorWhat::ArithmeticError(message.to_string())
                        ))?;
                        let result = match instruction.code
                            {
                                Code::CompareLess => order.is_lt(),
                                Code::CompareLessEqual => !order.is_gt(),
                                Code::CompareGreater => order.is_gt(),
                                Code::CompareGreaterEqual => !order.is_lt(),
                                _ => unreachable!(),
                            };
                        Self::push(&mut stack, Value::Boolean(result));
                    },

                Code::MathAdd | Code::MathSubtract | Code::MathMultiply
                | Code::MathDivide | Code::MathModulo =>
                    {
                        let error = |message: &str| InterpreterError::new(
                            location.clone(),
                            ErrorWhat::ArithmeticError(message.to_string())
                        );
                        let mut rhs = Self::pop(&location, &mut stack)?;
                        let mut lhs = Self::pop(&location, &mut stack)?;
                        self.consume_success(&location, &lhs)?;
                        self.consume_success(&location, &rhs)?;
                        let mut named = Vec::new();
                        while matches!(lhs, Value::Named(_)) || matches!(rhs, Value::Named(_))
                        {
                            let (Value::Named(left), Value::Named(right)) = (&lhs, &rhs) else
                            { return Err(error("Named arithmetic requires matching types; \
                                convert explicitly")); };
                            if left.definition.id != right.definition.id
                            { return Err(error("Named arithmetic requires matching types; \
                                convert explicitly")); }
                            named.push(left.definition.id);
                            (lhs, rhs) = (left.value.clone(), right.value.clone());
                        }
                        let wrap = |mut value: Value| -> InterpreterResult<Value>
                            {
                                for id in named.iter().rev()
                                {
                                    value = self.scope().types.coerce(*id, value)
                                        .map_err(|message| error(&message))?;
                                }
                                Ok(value)
                            };
                        if    matches!(instruction.code, Code::MathAdd)
                           && let (Value::String(left, _), Value::String(right, _)) = (&lhs, &rhs)
                        {
                            let text = format!("{}{}", left, right);
                            Self::push(&mut stack, wrap(Value::from_string(text))?);
                            instruction_pointer += 1;
                            continue;
                        }
                        if let Some(message) = lhs
                            .arithmetic_error()
                            .or_else(|| rhs.arithmetic_error())
                        {
                            return Err(error(message));
                        }
                        if matches!(lhs, Value::Float(_, _)) || matches!(rhs, Value::Float(_, _))
                        {
                            let lhs = lhs.arithmetic_float()
                                .ok_or_else(|| error("Non-finite floating-point operand"))?;
                            let rhs = rhs.arithmetic_float()
                                .ok_or_else(|| error("Non-finite floating-point operand"))?;
                            if    rhs == 0.0
                               && matches!(instruction.code, Code::MathDivide | Code::MathModulo)
                            { return Err(error("Division or remainder by zero")); }
                            let result = match instruction.code
                                {
                                    Code::MathAdd => lhs + rhs,
                                    Code::MathSubtract => lhs - rhs,
                                    Code::MathMultiply => lhs * rhs,
                                    Code::MathDivide => lhs / rhs,
                                    Code::MathModulo => lhs % rhs,
                                    _ => unreachable!(),
                                };
                            if !result.is_finite()
                            { return Err(error("Floating-point overflow")); }
                            Self::push(&mut stack, wrap(Value::Float(result, None))?);
                            instruction_pointer += 1;
                            continue;
                        }
                        let (Value::Integer(lhs), Value::Integer(rhs)) = (lhs, rhs)
                        else { unreachable!("Arithmetic operands have been checked") };
                        if    rhs == 0
                           && matches!(instruction.code, Code::MathDivide | Code::MathModulo)
                        {
                            return Err(error("Division or remainder by zero"));
                        }
                        let result = match instruction.code
                            {
                                Code::MathAdd => lhs.checked_add(rhs),
                                Code::MathSubtract => lhs.checked_sub(rhs),
                                Code::MathMultiply => lhs.checked_mul(rhs),
                                Code::MathDivide => lhs.checked_div(rhs),
                                Code::MathModulo => lhs.checked_rem(rhs),
                                _ => unreachable!()
                            }.ok_or_else(|| error("Integer overflow"))?;
                        Self::push(&mut stack, wrap(Value::Integer(result))?);
                    }
            }

            instruction_pointer += 1;
        }

        Ok(())
    }

    pub fn set_variable(&mut self, name: &str, value: Value)
    {
        let _ = self.scope_mut().variables.create(name.to_string(), ScopedValue
            {
                value,
                type_id: None,
                exported: ValueVisibility::Private,
                reference: None,
            });
    }

    pub fn variable_names(&self) -> Vec<String>
    {
        let mut names: Vec<String> = self.scope().variables.names()
            .chain(self.special_vars.keys().copied()).map(str::to_string).collect();
        names.sort();
        names.dedup();
        names
    }

    pub fn evaluate_variable(&mut self, name: &str) -> InterpreterResult<String>
    {
        if self.variable_binding(name).is_none() && !self.special_vars.contains_key(name)
        {
            return Ok(String::new());
        }

        let location = Location::new(name, 1, 1);
        let value = self.read_variable(name, &location)?;

        self.interpolate_string_at(&location, &value.as_text(), &[], true, false)
    }

    /**
     * Read a filesystem setting, expanding its home-shortened form.
     */
    pub fn evaluate_path_variable(&self, name: &str) -> InterpreterResult<String>
    {
        if self.variable_binding(name).is_none() && !self.special_vars.contains_key(name)
        {
            return Ok(String::new());
        }
        let value = self.read_raw_variable(name, &Location::new(name, 1, 1))?.as_text();

        Ok(if name == "$PATH"
            {
                self.eval_path_list_from(&value)
            }
            else
            {
                self.eval_path_from(&value)
            })
    }

    pub fn history_settings(&self) -> InterpreterResult<(Option<PathBuf>, usize)>
    {
        let location = Location::new("$history_path", 1, 1);
        let invalid = |message: &str| InterpreterError::new(
            location.clone(),
            ErrorWhat::InvalidOperand(message.into())
        );
        let path = match self.read_raw_variable("$history_path", &location)?
        {
            Value::None => None,
            Value::String(path, _) if !path.is_empty() =>
                {
                    let path = PathBuf::from(self.eval_path_from(&path));
                    Some(if path.is_absolute() { path } else
                        { current_dir().map_err(|error| invalid(&error.to_string()))?.join(path) })
                },
            _ => return Err(invalid("History path must be a nonempty String or ()")),
        };
        let location = Location::new("$history_limit", 1, 1);
        let limit = match self.read_raw_variable("$history_limit", &location)?
        {
            Value::Integer(limit) if limit >= 0 => usize::try_from(limit).ok(),
            _ => None,
        }.filter(|limit| *limit != usize::MAX).ok_or_else(|| InterpreterError::new(
            location,
            ErrorWhat::InvalidOperand(
                "History limit must be a nonnegative, representable Integer".into()
            )
        ))?;
        Ok((path, limit))
    }

    /**
     * Preserve value types while shortening path strings, including array entries.
     */
    fn eval_value_paths_to(&self, value: Value) -> Value
    {
        match value
        {
            Value::Named(mut item) =>
                {
                    let value = self.eval_value_paths_to(item.value.clone());
                    Rc::make_mut(&mut item).value = value;
                    Value::Named(item)
                },
            Value::Struct(mut item) =>
                {
                    let fields = item
                        .fields
                        .iter()
                        .cloned()
                        .map(|value| self.eval_value_paths_to(value))
                        .collect();
                    Rc::make_mut(&mut item).fields = fields;
                    Value::Struct(item)
                },
            Value::String(path, executable) => Value::String(self.eval_path_to(&path), executable),
            Value::Array(values) =>
                {
                    let type_id = values.type_id;
                    let result = Value::from_array(Rc::unwrap_or_clone(values).into_iter()
                        .map(|value| self.eval_value_paths_to(value)).collect());
                    if let Some(id) = type_id { result.with_collection_type(id) } else { result }
                },
            Value::ArgumentExpansion(values) =>
                Value::from_argument_expansion(Rc::unwrap_or_clone(values).into_iter()
                    .map(|value| self.eval_value_paths_to(value)).collect()),
            Value::HashMap(values) =>
                {
                    let type_id = values.type_id;
                    let result = Value::from_hash_map(Rc::unwrap_or_clone(values).into_iter()
                        .map(|(key, value)| (key, self.eval_value_paths_to(value))).collect());
                    if let Some(id) = type_id { result.with_collection_type(id) } else { result }
                },
            value => value
        }
    }

    fn read_variable(&self, name: &str, location: &Location) -> InterpreterResult<Value>
    {
        Ok(self.eval_value_paths_to(self.read_raw_variable(name, location)?))
    }

    pub(crate) fn read_projection(location: &Location, mut value: Value, indexes: &[Value],
                       fields: &[Value]) -> InterpreterResult<Value>
    {
        for (index, field) in indexes.iter().zip(fields)
        {
            if matches!(field, Value::None)
            {
                let Value::Named(item) = value else
                { return Err(InterpreterError::new(
                    location.clone(),
                    ErrorWhat::InvalidOperand("Named receiver changed type".into())
                )); };
                value = item.value.clone();
                continue;
            }
            value = if matches!(field, Value::Boolean(true))
                {
                    match value.underlying()
                    {
                        Value::Enum(item) if index.as_text() == "index" =>
                            Value::Integer(item.variant as i64),
                        Value::Type(item) if index.as_text() == "name" =>
                            Value::from_string(item.name.clone()),
                        Value::Struct(item) =>
                            item.fields[Self::struct_field_index(location, item, index)?].clone(),
                        _ => return Err(InterpreterError::new(
                            location.clone(),
                            ErrorWhat::InvalidOperand(
                                format!(
                                    "Cannot access a field on {}",
                                    value.type_name()
                                )
                            )
                        )),
                    }
                }
                else { Self::get_element(location, &value, index)? };
        }
        Ok(value)
    }

    fn read_reference(
        &self,
        location: &Location,
        reference: &ValueReference,
    ) -> InterpreterResult<Value>
    {
        let binding = reference.root.borrow();
        if matches!(binding.value, Value::None)
            && binding
                .type_id
                .is_some_and(|id| self.scope().types.validate(id, &Value::None).is_err())
        {
            return Err(InterpreterError::new(
                location.clone(),
                ErrorWhat::InvalidOperand("Binding has not been initialized".into()),
            ));
        }
        drop(binding);
        let root = reference.root.borrow().value.clone();
        for (depth, id) in &reference.constraints
        {
            let value = Self::read_projection(location, root.clone(),
                &reference.indexes[..*depth], &reference.fields[..*depth])?;
            self.scope().types.validate(*id, &value).map_err(|message| InterpreterError::new(
                location.clone(),
                ErrorWhat::InvalidOperand(
                    format!("Type error for receiver: {}", message)
                )
            ))?;
        }
        let value = Self::read_projection(location, root, &reference.indexes, &reference.fields)?;
        self.scope().types.validate_value(&value)
            .map_err(|message| protocol_error(location, &format!("Type error: {}", message)))?;
        if let Value::String(_, Executable::Method(method)) = &value
            && let Some(property) = &method.receiver.property
        { property.fresh.set(false); }
        Ok(value)
    }

    pub(super) fn write_variable(
        &mut self,
        location: &Location,
        name: &str,
        mut value: Value,
    ) -> InterpreterResult<()>
    {
        let invalid = |message| InterpreterError::new(
            location.clone(),
            ErrorWhat::InvalidOperand(message)
        );
        let variable = self.variable_binding(name)
            .ok_or_else(|| invalid("Variable not found for SetVariable instruction.".to_string()))?;
        let expected = variable.borrow().type_id;
        self.prepare_failure_value(location, &mut value, expected)?;
        self.check_failure_write(location, &variable, &value)?;
        if let Some(id) = expected
        {
            value = self.scope().types.coerce(id, value).map_err(|message|
                invalid(format!("Type error for '{}': {}", name, message)))?;
        }
        drop(variable);
        let reference = self.variable_reference(name).unwrap();
        self.write_receiver(location, name, &reference, value)
    }

    fn write_receiver(
        &mut self,
        location: &Location,
        name: &str,
        reference: &ValueReference,
        mut value: Value,
    ) -> InterpreterResult<()>
    {
        crate::runtime::tasks::check_write(Rc::as_ptr(&reference.root) as usize).map_err(
            |error| InterpreterError::new(location.clone(), ErrorWhat::ReceiverBusy(error.message)),
        )?;
        let invalid = |message| InterpreterError::new(
            location.clone(),
            ErrorWhat::InvalidOperand(message)
        );
        if self.exceptions.reporting
        {
            let key = Rc::as_ptr(&reference.root) as usize;
            let local = self.exceptions.bindings.get(&key).is_some_and(|owner| {
                self.exceptions
                    .regions
                    .iter()
                    .position(|region| region == owner)
                    .is_some_and(|position| position >= self.exceptions.report_floor)
            });
            if !local
            {
                return Err(protocol_error(
                    location,
                    "Reporting cannot mutate module or caller bindings",
                ));
            }
        }
        let original = reference.root.borrow().value.clone();
        if reference.indexes.is_empty()
        {
            self.check_failure_write(location, &reference.root, &value)?;
        }
        else
        {
            self.consume_success(location, &value)?;
            self.consume_success(location, &original)?;
        }
        let root = reference.root.borrow();
        let root_type = root.type_id;
        if let Some(id) = Self::read_projection(location, root.value.clone(),
            &reference.indexes, &reference.fields).ok().and_then(|value| value.collection_type())
        {
            value = self.scope().types.coerce(id, value)
                .map_err(|message| invalid(format!("Type error for '{}': {}", name, message)))?
                .with_collection_type(id);
        }
        let mut updated = root.value.clone();
        drop(root);
        self.set_element(location, &mut updated, &reference.indexes, &reference.fields, value)?;
        updated = self.scope().types.coerce_value(updated)
            .map_err(|message| invalid(format!("Type error: {}", message)))?;
        if let Some(id) = root_type
        {
            updated = self.scope().types.coerce(id, updated)
                .map_err(|message| invalid(format!("Type error for '{}': {}", name, message)))?;
        }
        for (depth, id) in &reference.constraints
        {
            let value = Self::read_projection(location, updated.clone(),
                &reference.indexes[..*depth], &reference.fields[..*depth])?;
            let value = self.scope().types.coerce(*id, value)
                .map_err(|message| invalid(format!("Type error for receiver: {}", message)))?;
            self.set_element(location, &mut updated, &reference.indexes[..*depth],
                &reference.fields[..*depth], value)?;
        }
        // Receiver promotion must also satisfy the original owner's constraints.
        self.scope().types.validate_value(&updated)
            .map_err(|message| invalid(format!("Type error: {}", message)))?;
        if let Some(id) = root_type
        {
            self.scope().types.validate(id, &updated)
                .map_err(|message| invalid(format!("Type error for '{}': {}", name, message)))?;
        }
        for (depth, id) in &reference.constraints
        {
            let value = Self::read_projection(location, updated.clone(),
                &reference.indexes[..*depth], &reference.fields[..*depth])?;
            self.scope().types.validate(*id, &value)
                .map_err(|message| invalid(format!("Type error for receiver: {}", message)))?;
        }
        if let Some(property) = &reference.property
        {
            let setter = property.setter.as_ref().ok_or_else(||
                protocol_error(location, &format!("Property '{}' is read-only", property.name)))?;
            let owner = self.read_reference(location, &property.getter.receiver)?;
            let candidate = ValueReference::temporary(owner);
            let setter = BoundMethod
                {
                    receiver: candidate.clone(), name: setter.name.clone(),
                    definition: setter.definition.clone(), snapshot: setter.snapshot.clone(),
                };
            let previous = self.last_result.take();
            let result = self.execute_method(location, &setter, &[updated.clone()]);
            self.last_result = previous;
            result?;
            let owner = candidate.root.borrow().value.clone();
            self.write_receiver(location, name, &property.getter.receiver, owner)?;
        }
        reference.root.borrow_mut().value = updated;
        self.commit_failure_write(&reference.root, &original);
        Ok(())
    }

    pub(super) fn read_raw_variable(&self, name: &str, location: &Location)
        -> InterpreterResult<Value>
    {
        if name == "$"
        {
            return self.pipeline_inputs.last().cloned().ok_or_else(|| InterpreterError::new(
                location.clone(),
                ErrorWhat::InvalidOperand(
                    "Bare $ is only available inside a pipeline stage".into()
                )
            ));
        }
        if let Some(reference) = self.variable_reference(name)
        {
            self.read_reference(location, &reference)
        }
        else if let Some(value) = self.special_vars.get(name).cloned()
        {
            Ok(value(self)?)
        }
        else
        {
            Err(InterpreterError::new(
                location.clone(),
                ErrorWhat::InvalidOperand(
                    format!("Variable '{}' not found.", name)
                )
            ))
        }
    }

    pub(super) fn bind_executable(&self, value: Value) -> Value
    {
        if let Value::String(name, Executable::Yes) = &value
        {
            if let Some(function) = self.module_native_function(name)
            { return Value::String(name.clone(), Executable::Native(function)); }
            let function = self.module_function(name);
            if let Some(function) = function
            { return Value::String(name.clone(), Executable::Function(function)); }
        }
        value
    }

    pub(super) fn execute_value(
        &mut self,
        location: &Location,
        value: Value,
        args: Vec<Value>,
    ) -> InterpreterResult<()>
    {
        if    let Value::Named(item) = &value
           && (item.value.is_callable() || item.value.is_external())
        { return self.execute_value(location, item.value.clone(), args); }
        if let Value::Closure(closure) = &value
        {
            let home = closure.function.functions.borrow().scope.clone();
            let Some(scope) = self.scopes.get_mut(&home) else
            {
                return Err(InterpreterError::new(
                    location.clone(),
                    ErrorWhat::ModuleError("The defining module failed to load".into())
                ));
            };
            let mut variables = scope.variables.global_bindings();
            variables.overlay(&closure.captures);
            // Prompt context is temporary and takes precedence over closure captures.
            if let Some(binding) = &self.prompt_widgets
            {
                variables.push_scope();
                variables.import("$rendered_widgets".into(), binding.clone());
            }
            let previous = replace(&mut scope.variables, variables);
            let result = self.execute_function(location, "<anonymous>", &closure.function, &args);
            self.scopes.get_mut(&home).unwrap().variables = previous;
            return result;
        }
        if let Value::Callable(callable) = &value
        {
            let TypeKind::Function(parameters, result) = &callable.prototype.kind
                else { unreachable!("Callable values carry a function prototype"); };
            let error = |message| InterpreterError::new(
                location.clone(),
                ErrorWhat::InvalidOperand(format!("Function prototype: {}", message))
            );
            if args.len() != parameters.len()
            {
                return Err(error(format!("Expected {} arguments, got {}",
                    parameters.len(), args.len())));
            }
            let mut checked = Vec::new();
            for (index, (id, mut value)) in parameters.iter().zip(args).enumerate()
            {
                self.prepare_failure_value(location, &mut value, Some(*id))?;
                checked.push(self.scope().types.coerce(*id, value).map_err(|message|
                    error(format!("Argument {}: {}", index + 1, message)))?);
            }
            let args = checked;
            self.execute_value(location, callable.target.clone(), args)?;
            if self.halted { return Ok(()); }
            let mut value = self.last_result.take().unwrap_or(Value::None);
            self.prepare_failure_value(location, &mut value, Some(*result))?;
            let value = self.scope().types.coerce(*result, value)
                .map_err(|message| error(format!("Return value: {}", message)))?;
            self.last_result = Some(value);
            return Ok(());
        }
        if let Value::String(name, Executable::Function(function)) = value
        { return self.execute_function(location, &name, &function, &args); }
        if let Value::String(_, Executable::Native(function)) = value
        { return self.execute_native(location, &function, &args); }
        if let Value::String(_, Executable::Method(method)) = value
        { return self.execute_method(location, &method, &args); }
        if let Value::String(name, Executable::External) = value
        { return self.execute_external(location, self.eval_path_from(&name), args); }
        self.execute(location, Self::command_name(location, value)?, args)
    }

    pub(super) fn resolve_method(&self, value: &Value, receiver: ValueReference,
                      name: &str, snapshot: &Value) -> Option<BoundMethod>
    {
        if name == "new" { return None; }
        let mut method_types = Vec::new();
        let mut underlying = value;
        while let Value::Named(item) = underlying
        {
            method_types.push(item.definition.id);
            underlying = &item.value;
        }
        let receiver_type = if name == "next_item"
            { self.scope().types.inferred_value_type(underlying) }
            else { self.scope().types.value_type(underlying) };
        let base_types = self.scope().types.method_types(receiver_type);
        let base_type = base_types[0];
        method_types.extend(base_types);
        for id in method_types
        {
            let key = method_key(id, name);
            let bound = match snapshot
                {
                    Value::HashMap(methods) => methods.get(&MapKey::String(key.clone())),
                    _ => None,
                };
            let function = if let Some(Value::String(_, Executable::Function(function))) = bound
                { Some(function.clone()) }
                else
                {
                    self.scope().lexical_function(&key).or_else(||
                        {
                            if matches!(bound, Some(Value::None))
                            { self.scope().imported_methods.get(&key).cloned() }
                            else { None }
                        }).or_else(|| self.module_method(id, &key))
                };
            let definition = if let Some(function) = function
                { MethodDefinition::User(function) }
                else if matches!(self.scope().types.get(id).kind, TypeKind::Named(_))
                { continue; }
                else if let Some(method) = self.scope().types.direct_method(
                    if id == base_type { receiver_type } else { id }, name)
                { MethodDefinition::Builtin(method) }
                else { continue; };
            let mut receiver = receiver.clone();
            let mut underlying = value;
            while let Value::Named(item) = underlying
            {
                if item.definition.id == id || Some(id) == self.scope().types.builtin_id("any")
                { break; }
                receiver.indexes.push(Value::None);
                receiver.fields.push(Value::None);
                underlying = &item.value;
            }
            return Some(BoundMethod
                { receiver, name: name.to_string(), definition, snapshot: snapshot.clone() });
        }
        None
    }

    pub(super) fn execute_method(
        &mut self,
        location: &Location,
        method: &BoundMethod,
        arguments: &[Value],
    ) -> InterpreterResult<()>
    {
        let native_receiver = matches!(
            self.read_reference(location, &method.receiver)?
                .underlying(),
            Value::Native(_)
        );
        let _pin = if !native_receiver && matches!(method.definition, MethodDefinition::User(_))
        {
            Some(
                crate::runtime::tasks::pin_receiver(Rc::as_ptr(&method.receiver.root) as usize)
                    .map_err(|error| {
                        InterpreterError::new(
                            location.clone(),
                            ErrorWhat::ReceiverBusy(error.message),
                        )
                    })?,
            )
        }
        else
        {
            None
        };
        let property = method.receiver.property.clone();
        if let Some(property) = &property
            && !property.active.get() && !property.fresh.replace(false)
        {
            let previous = self.last_result.take();
            let result = self.execute_method(location, &property.getter, &[]);
            let value = self.last_result.take().unwrap_or(Value::None);
            self.last_result = previous;
            result?;
            method.receiver.root.borrow_mut().value = value;
        }
        let active = property.as_ref().map(|property| property.active.replace(true));
        let result = self.execute_method_body(location, method, arguments);
        if let Some(property) = property { property.active.set(active.unwrap()); }
        result.map_err(|error|
            {
                let native = matches!(method.definition, MethodDefinition::Builtin(_));
                // User methods already acquired a frame in execute_function_with_receiver.
                if !native && error.trace.call_stack.last().is_some_and(|frame|
                    frame.location == *location && frame.name.ends_with(
                        &format!("::{}", method.name)))
                { return error; }
                let receiver_type = match &method.definition
                    {
                        MethodDefinition::Builtin(definition) => Some(definition.receiver_type),
                        MethodDefinition::User(function) => function.receiver_type,
                    };
                let name = receiver_type.map_or_else(|| method.name.clone(), |id|
                    format!("{}::{}", self.scope().types.get(id).name, method.name));
                error.through_call(&name, location, native)
            })
    }

    fn execute_method_body(
        &mut self,
        location: &Location,
        method: &BoundMethod,
        arguments: &[Value],
    ) -> InterpreterResult<()>
    {
        if    let MethodDefinition::User(function) = &method.definition
           && (is_conversion_method(&method.name) || method.name == "json_codec")
           && function.receiver_type.is_some()
        {
            return Err(InterpreterError::new(
                location.clone(),
                ErrorWhat::InvalidOperand(
                    "Conversion methods have no receiver; use Target(value)".into()
                )
            ));
        }
        let mut receiver =
            self.eval_value_paths_to(self.read_reference(location, &method.receiver)?);
        let (minimum, maximum) = match &method.definition
            {
                MethodDefinition::Builtin(definition) =>
                    (definition.argument_count, Some(definition.argument_count)),
                MethodDefinition::User(function) =>
                    (function.minimum_arguments.saturating_sub(1),
                     (!function.variadic).then_some(function.arguments.len() - 1)),
            };
        if arguments.len() < minimum || maximum.is_some_and(|max| arguments.len() > max)
        {
            let expected = match maximum
                {
                    None => format!("at least {}", minimum),
                    Some(max) if max != minimum => format!("{} to {}", minimum, max),
                    Some(max) => max.to_string(),
                };
            return Err(InterpreterError::new(
                location.clone(),
                ErrorWhat::ArgumentMismatch(
                    format!(
                        "Method '{}.{}' expected {} arguments, but got {}",
                        receiver.type_name(),
                        method.name,
                        expected,
                        arguments.len()
                    )
                )
            ));
        }
        match &method.definition
        {
            MethodDefinition::Builtin(definition) =>
                {
                    for argument in arguments { self.consume_success(location, argument)?; }
                    let mut value = (definition.body)(&mut receiver, arguments)
                        .map_err(|error|
                            {
                                let mut failure = InterpreterError::new(
                                    location.clone(),
                                    ErrorWhat::InvalidOperand(error.message)
                                );
                                failure.trace.native_location = Some(error.location);
                                failure
                            })?;
                    if definition.name == "properties"
                    { self.add_property_methods(&receiver, method, &mut value); }
                    if definition.mutates_receiver
                    { self.write_receiver(location, "$self", &method.receiver, receiver)?; }
                    self.last_result = Some(value);
                    Ok(())
                },
            MethodDefinition::User(function) =>
                {
                    let mut values = Vec::with_capacity(arguments.len() + 1);
                    values.push(receiver.clone());
                    values.extend_from_slice(arguments);
                    self.execute_function_with_receiver(location,
                        &format!("{}::{}", receiver.type_name(), method.name),
                        function, &values, Some(&method.receiver))
                },
        }
    }

    fn can_execute(&self, executable: &str) -> bool
    {
        if    (executable.contains("::") && self.scope().types.module_names
                .contains(executable.split("::").next().unwrap()))
           || self.module_native_function(executable).is_some()
           || self.module_alias_in(&self.current_scope, executable).is_some()
           || self.module_function(executable).is_some()
        {
            return true;
        }

        let is_executable = |path: &Path|
            {
                let Ok(metadata) = path.metadata() else { return false; };
                if !metadata.is_file()
                {
                    return false;
                }

                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    metadata.permissions().mode() & 0o111 != 0
                }

                #[cfg(not(unix))]
                {
                    true
                }
            };

        if executable.contains('/')
        {
            return is_executable(Path::new(&self.eval_path_from(executable)));
        }

        // Command uses the child's exported PATH, or the system default when it is absent.
        let path = self.scope().variables.get("$PATH")
            .filter(|value| value.exported == ValueVisibility::Exported)
            .map(|value| self.eval_path_list_from(&value.value.as_text()))
            .unwrap_or_else(|| "/bin:/usr/bin".to_string());

        split_paths(&path).any(|directory| is_executable(&directory.join(executable)))
    }

    pub(super) fn execute_function(&mut self,
                        location: &Location,
                        name: &str,
                        function: &FunctionRef,
                        args: &[Value]) -> InterpreterResult<()>
    {
        self.execute_function_with_receiver(location, name, function, args, None)
    }

    fn execute_function_with_receiver(
        &mut self,
        location: &Location,
        name: &str,
        function: &FunctionRef,
        args: &[Value],
        receiver: Option<&ValueReference>,
    ) -> InterpreterResult<()>
    {
        if self.exceptions.frames.len() >= 512
        {
            return Err(InterpreterError::new(
                location.clone(),
                ErrorWhat::InvalidOperand("Function call depth limit exceeded".into()),
            ));
        }
        let previous_transfer = self.exceptions.transfer.take();
        let display = function
            .receiver_type
            .map(|id| {
                let binding = function.functions.borrow();
                let method = binding
                    .function_name
                    .as_deref()
                    .unwrap_or(name)
                    .rsplit(':')
                    .next()
                    .unwrap_or(name);
                format!("{}::{}", self.scope().types.get(id).name, method)
            })
            .unwrap_or_else(|| name.into());
        self.exceptions.frames.push(CallFrame {
            name: display,
            location: location.clone(),
            native: false,
            native_location: None,
        });
        self.exceptions.returns.push(function.return_type);
        self.exceptions.callers.push(self.current_scope.clone());
        let mut result = self.execute_function_body(location, name, function, args, receiver);
        let converts_error = function.receiver_type.is_some_and(|id| {
            self.scope().types.has_error_type(id) && self.scope().types.success_type(id).is_none()
        }) && function
            .functions
            .borrow()
            .function_name
            .as_ref()
            .is_some_and(|name| name.rsplit(':').next().is_some_and(is_conversion_method));
        if converts_error
        {
            self.conversion_causes(location, args, &mut result);
        }
        self.exceptions.returns.pop();
        self.exceptions.callers.pop();
        self.exceptions.frames.pop();
        self.exceptions.transfer = previous_transfer;
        result.map_err(|error|
                {
                    let display = function.receiver_type.map(|id|
                        {
                            let binding = function.functions.borrow();
                            let method = binding.function_name.as_deref().unwrap_or(name)
                                .rsplit(':').next().unwrap_or(name);
                            format!("{}::{}", self.scope().types.get(id).name, method)
                        }).unwrap_or_else(|| name.into());
                    error.through_call(&display, location, false)
                })
    }

    fn execute_function_body(
        &mut self,
        location: &Location,
        name: &str,
        function: &FunctionRef,
        args: &[Value],
        receiver: Option<&ValueReference>,
    ) -> InterpreterResult<()>
    {
        if    args.len() < function.minimum_arguments
           || !function.variadic
           && args.len() > function.arguments.len()
        {
            let expected = if function.variadic
            {
                format!("at least {}", function.minimum_arguments)
            }
            else if function.minimum_arguments == function.arguments.len()
            {
                function.arguments.len().to_string()
            }
            else
            {
                format!(
                    "{} to {}",
                    function.minimum_arguments,
                    function.arguments.len()
                )
            };
            return Err(InterpreterError::new(
                location.clone(),
                ErrorWhat::ArgumentMismatch(
                    format!(
                        "Function {} expected {} arguments, but got {}.",
                        name,
                        expected,
                        args.len()
                    )
                )
            ));
        }

        let registry = &self.scope().types;
        let dependent = function.argument_types.iter().flatten()
            .chain(function.return_type.iter()).any(|id| registry.has_receiver_type(*id));
        let (value_type, key_type) = if dependent
            {
                let reference = receiver.ok_or_else(|| InterpreterError::new(
                    location.clone(),
                    ErrorWhat::ArgumentMismatch(
                        "Receiver-dependent annotations require a bound method receiver".into()
                    )
                ))?;
                let value = self.read_reference(location, reference)?;
                (registry.receiver_member_type(location, reference, &value, "value_type"),
                 registry.receiver_member_type(location, reference, &value, "key_type"))
            }
            else { (None, None) };
        let parameter_types: Vec<_> = function.argument_types.iter().map(|id|
            id.map(|id| registry.specialize_receiver_type(id, value_type, key_type))).collect();
        let return_type = function.return_type
            .map(|id| registry.specialize_receiver_type(id, value_type, key_type));

        let factory = function.receiver_type.is_some()
            && function.functions.borrow().function_name.as_ref()
                .is_some_and(|name| name.ends_with(":new"));
        if factory && self.active_factories.contains(&function.receiver_type.unwrap())
        { return Err(protocol_error(location, "Recursive constructor call")); }

        let home = function.functions.borrow().scope.clone();
        if !self.scopes.contains_key(&home)
        {
            return Err(InterpreterError::new(
                location.clone(),
                ErrorWhat::ModuleError("The defining module failed to load".into())
            ));
        }
        let caller_scope = replace(&mut self.current_scope, home);
        let caller = self.scope_mut().enter_function(function.functions.clone());

        let origin = function.code.iter().find_map(|instruction|
            instruction.location.as_ref().map(|location| location.origin.to_string()))
            .unwrap_or_else(|| location.origin.to_string());
        if factory { self.active_factories.push(function.receiver_type.unwrap()); }
        let allowed = !self.active_factories.is_empty();
        let previous_candidates = replace(&mut self.scope_mut().types.allow_candidates, allowed);
        let owner = function.functions.borrow().protocol_owner;
        self.protocol_context.push((owner, origin));
        let call_result = self.execute_instructions_with_arguments(
            &function.code, args, receiver, &parameter_types, true);
        self.protocol_context.pop();
        self.scope_mut().types.allow_candidates = previous_candidates;
        if factory { self.active_factories.pop(); }

        self.scope_mut().exit_function(caller);
        self.current_scope = caller_scope;

        call_result?;
        if !self.halted && let Some(id) = return_type
        {
            let mut value = self.last_result.take().unwrap_or(Value::None);
            if factory || self.active_factories.is_empty() { seal_constructed(&mut value); }
            self.last_result = Some(self.scope().types.coerce(id, value).map_err(|message|
                InterpreterError::new(
                    location.clone(),
                    ErrorWhat::ReturnContract(
                        format!(
                            "Type error for return value of '{}': {}",
                            name,
                            message
                        ), Rc::as_ptr(function) as usize
                    )
                ))?);
        }
        Ok(())
    }

    fn find_and_execute_function(&mut self,
                        location: &Location,
                        executable: &str,
                        args: &[Value]) -> InterpreterResult<bool>
    {
        if let Some(function) = self.module_function(executable)
        {
            self.execute_function(location, executable, &function, args)?;
            return Ok(true);
        }

        if executable.contains("::") && self.scope().types.module_names
            .contains(executable.split("::").next().unwrap())
        {
            return Err(InterpreterError::new(
                location.clone(),
                ErrorWhat::ModuleError(
                    format!("Module function '{}' is unavailable", executable)
                )
            ));
        }
        Ok(false)
    }

    fn execute(&mut self,
               location: &Location,
               executable: String,
               args: Vec<Value>) -> InterpreterResult<()>
    {
        let (executable, resolved_args, home) = self.resolve_alias(location, &executable)?;
        let caller = replace(&mut self.current_scope, home);
        let result = self.execute_resolved(location, executable, resolved_args, args);
        self.current_scope = caller;
        result
    }

    fn execute_native(
        &mut self,
        location: &Location,
        function: &NativeFunction,
        args: &[Value],
    ) -> InterpreterResult<()>
    {
        if function.name != "__report_error"
        {
            for argument in args
            {
                self.consume_success(location, argument)?;
            }
        }
        if self.exceptions.cleanup.is_some() && function.name == "exit"
        {
            return Err(InterpreterError::new(
                location.clone(),
                ErrorWhat::CleanupControl,
            ));
        }
        let http_metadata = function.name == "__http" && args.first().is_some_and(|value|
            matches!(value, Value::String(command, _) if matches!(command.as_str(), "status" |
                "headers" | "url" | "diagnostic_complete" | "diagnostic_reason" |
                    "diagnostic_bytes" | "diagnostic_received")));
        let pure = http_metadata
            || matches!(
                function.name,
                "__json"
                    | "__json_bind"
                    | "__json_apply"
                    | "__http_apply"
                    | "__bytes"
                    | "__http_header"
            )
            || FACTORIES.iter().any(|(name, _, _)| *name == function.name);
        let stream_close = matches!(function.name, "__stream" | "__json_reader")
            && args.first().is_some_and(
                |value| matches!(value, Value::String(command, _) if command == "close"),
            );
        let cleanup = (matches!(
            function.name,
            "__io_close"
                | "__tcp_close"
                | "__http_close"
                | "__socket_close"
                | "__server_close"
                | "terminal_close"
        ) || stream_close)
            && !self.exceptions.reporting
            && self.exceptions.json_calls == 0;
        if (self.exceptions.reporting || self.exceptions.cleanup.is_some())
            && !pure
            && !cleanup
            && function.name != "__report_error"
        {
            return Err(InterpreterError::new(
                location.clone(),
                ErrorWhat::UnsupportedSuspension(format!(
                    "Native '{}' is not admitted during cleanup/reporting",
                    function.name
                )),
            ));
        }
        if crate::runtime::tasks::managed()
            && function.operation.is_none()
            && !pure
            && !cleanup
            && !matches!(function.name, "__stream" | "__report_error")
        {
            return Err(InterpreterError::new(
                location.clone(),
                ErrorWhat::UnsupportedSuspension(format!(
                    "Native '{}' has no managed suspension contract",
                    function.name
                )),
            ));
        }
        let result = if let Some(operation) = function.operation
        {
            let outcome = operation(self, location, args);
            let outcome = if matches!(outcome, NativeOutcome::Wait(_))
                && !crate::runtime::tasks::active()
            {
                NativeOutcome::Failed(InterpreterError::new(
                    location.clone(),
                    ErrorWhat::UnsupportedSuspension(
                        "Native wait requires a running VM task".into(),
                    ),
                ))
            }
            else
            {
                outcome
            };
            let value = match outcome
            {
                NativeOutcome::Ready(value) => Ok(value),
                NativeOutcome::Failed(error) => Err(error),
                NativeOutcome::Wait(wait) =>
                {
                    let operation = self.exceptions.operation.clone().filter(|_| {
                        self.exceptions
                            .cleanup
                            .as_ref()
                            .is_none_or(|guard| guard.hook)
                    });
                    let cleanup_deadline = self
                        .exceptions
                        .cleanup
                        .as_ref()
                        .filter(|guard| !guard.hook)
                        .map(|guard| guard.deadline);
                    let future = if operation.is_some() || cleanup_deadline.is_some()
                    {
                        let mut future = wait.future;
                        Box::pin(async move {
                            let mut tick = tokio::time::interval(Duration::from_millis(10));
                            loop
                            {
                                tokio::select! {
                                    reply = &mut future => return reply,
                                    _ = tick.tick() =>
                                        {
                                            if cleanup_deadline.is_some_and(|deadline|
                                                std::time::Instant::now() >= deadline)
                                            { return
                                                Err(crate::runtime::tasks::NativeFault::new("CleanupTimeout", "Cleanup budget exhausted")); }
                                            if operation.as_ref().is_some_and(|(cancel, deadline)|
                                                cancel.iter().any(|flag|
                                                    flag.load(std::sync::atomic::Ordering::Acquire))
                                                || std::time::Instant::now() >= *deadline)
                                            { return
                                                Err(crate::runtime::tasks::NativeFault::new("OperationCancelled", "Operation cancelled")); }
                                        },
                                }
                            }
                        }) as crate::runtime::tasks::NativeFuture
                    }
                    else
                    {
                        wait.future
                    };
                    let reply = crate::runtime::tasks::wait(future);
                    if reply
                        .as_ref()
                        .err()
                        .is_some_and(|error| error.kind == "OperationCancelled")
                    {
                        return Err(InterpreterError::new(
                            location.clone(),
                            ErrorWhat::OperationCancelled,
                        ));
                    }
                    if reply
                        .as_ref()
                        .err()
                        .is_some_and(|error| error.kind == "CleanupTimeout")
                    {
                        return Err(InterpreterError::new(
                            location.clone(),
                            ErrorWhat::CleanupTimeout,
                        ));
                    }
                    if reply.as_ref().err().is_some_and(|error| {
                        matches!(error.kind.as_str(), "Interrupted" | "CleanupAbort")
                    })
                    {
                        let abort = reply
                            .as_ref()
                            .err()
                            .is_some_and(|error| error.kind == "CleanupAbort")
                            || self
                                .exceptions
                                .cleanup
                                .as_ref()
                                .is_some_and(|guard| guard.interrupted);
                        if let Some(guard) = self.exceptions.cleanup.as_mut()
                        {
                            guard.interrupted = true;
                        }
                        return Err(InterpreterError::new(
                            location.clone(),
                            if abort
                            {
                                ErrorWhat::CleanupAbort
                            }
                            else
                            {
                                ErrorWhat::Interrupted
                            },
                        )
                        .through_call(function.name, location, true));
                    }
                    (wait.finish)(self, reply)
                }
            };
            value.and_then(|mut value| {
                let native_created = crate::language::data::failure::occurrence(&value)
                    .is_none_or(|failure| failure.borrow().native_origin.is_some());
                self.activate_failure(location, &mut value, false)?;
                if native_created
                    && let Some(failure) = crate::language::data::failure::occurrence(&value)
                {
                    let mut failure = failure.borrow_mut();
                    let boundary = Location::new(file!(), line!() as usize, column!() as usize);
                    if failure.native_origin.is_none()
                    {
                        failure.native_origin = Some(boundary.clone());
                    }
                    if !failure.frames.first().is_some_and(|frame| {
                        frame.native && frame.name == function.name && frame.location == *location
                    })
                    {
                        failure.frames.insert(
                            0,
                            CallFrame {
                                name: function.name.into(),
                                location: location.clone(),
                                native: true,
                                native_location: Some(boundary),
                            },
                        );
                    }
                    failure.frames.truncate(128);
                }
                if let Value::Native(native) = value.underlying()
                    && native.0.operation()
                {
                    self.exceptions
                        .resources
                        .retain(|(_, native)| native.0.operation());
                    if self.exceptions.resources.len() >= 256
                    {
                        native.0.release();
                        value = self.io_fault(
                            "LimitError",
                            function.name,
                            "Too many active operation resources".into(),
                        );
                        self.activate_failure(location, &mut value, false)?;
                    }
                    else
                    {
                        let region = *self.exceptions.regions.last().unwrap_or(&0);
                        self.exceptions.resources.push((region, native.clone()));
                    }
                }
                self.last_result = Some(value);
                Ok(())
            })
        }
        else
        {
            (function.body)(self, location, args)
        };
        result
            .map_err(|error| error.through_call(function.name, location, true))
    }

    fn execute_resolved(&mut self, location: &Location, executable: String,
                        resolved_args: Vec<String>, args: Vec<Value>) -> InterpreterResult<()>
    {
        let executable = self.eval_path_from(&executable);
        let args: Vec<Value> = resolved_args
            .into_iter()
            .map(Value::from_string)
            .chain(args)
            .collect();

        if let Some(built_in) = self.module_native_function(executable.as_str())
        {
            return self.execute_native(location, &built_in, &args);
        }

        if self.find_and_execute_function(location, &executable, &args)?
        {
            // Make sure `last_result` as been updated by the executed function.
            if self.last_result.is_none()
            {
                return Err(InterpreterError::new(
                                location.clone(), ErrorWhat::NoResult));
            }

            return Ok(());
        }

        self.execute_external(location, executable, args)
    }

    fn execute_external(
        &mut self,
        location: &Location,
        executable: String,
        args: Vec<Value>,
    ) -> InterpreterResult<()>
    {
        let env_vars = self.exported_environment(location)?;

        for argument in &args
        {
            self.consume_success(location, argument)?;
        }
        if self.exceptions.cleanup.is_some()
            || self.exceptions.reporting
            || crate::runtime::tasks::managed()
        {
            return Err(InterpreterError::new(
                location.clone(),
                ErrorWhat::UnsupportedSuspension(
                    "External commands are not admitted during cleanup/reporting".into(),
                ),
            ));
        }
        let mut command = Command::new(&executable);

        command.args(args.iter().map(|argument| self.eval_path_from(&argument.as_text())))
            .env_clear().envs(env_vars);

        command.stdin(Stdio::inherit()).stdout(Stdio::inherit()).stderr(Stdio::inherit());
        let redirected_output = configure(&mut command, &self.redirections)
            .map_err(|error| Self::redirection_error(location, error))?;
        let status_result = if self.captured_stdout.is_some() && !redirected_output
            {
                command
                    .stdout(Stdio::piped())
                    .output()
                    .map(|output|
                        {
                            self.captured_stdout
                                .as_mut()
                                .expect("Capture buffer must be installed.")
                                .extend_from_slice(&output.stdout);

                            output.status
                        })
            }
            else
            {
                command.status()
            };

        let status = status_result
            .map_err(|error| InterpreterError::new(
                location.clone(),
                match error.kind()
                {
                    ErrorKind::NotFound =>
                        ErrorWhat::ExecutableNotFound(self.eval_path_to(&executable)),

                    _ => ErrorWhat::ExecutableIoError(
                        format!("'{}': {}", self.eval_path_to(&executable), error))
                }
            ))?;

        // Only acknowledge an interruption that actually terminated this child. A signal
        // arriving after a successful child's exit still belongs to the running script.
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            if status.signal() == Some(libc::SIGINT) && take_interrupt() && !self.interactive
            {
                return Err(InterpreterError::new(
                    location.clone(),
                    ErrorWhat::Interrupted,
                ));
            }
        }
        // Set the `last_result` to indicate the result of the executed command.
        let value = Value::from_status_code(status.code());
        self.last_result = Some(value);

        Ok(())
    }

    #[track_caller]
    pub(super) fn redirection_error(location: &Location, error: impl Display) -> InterpreterError
    {
        InterpreterError::new(location.clone(), ErrorWhat::RedirectionError(error.to_string()))
    }

    pub(super) fn begin_redirection(&mut self, location: &Location, stream: i64,
                          variable: bool, target: Value) -> InterpreterResult<()>
    {
        let Value::String(text, _) = target else
        {
            return Err(Self::redirection_error(location,
                "A redirection requires a string path or string variable"));
        };
        let mut redirect = Redirection::new(location.clone());
        let target_name = text.clone();
        let output = if variable
            {
                // Validate before running commands or replacing the binding.
                let binding = self.variable_binding(&text).ok_or_else(||
                    Self::redirection_error(location,
                        format!("Variable '{}' not found; declare it with let", text)))?;
                if let Some(id) = binding.borrow().type_id
                {
                    self.scope().types.validate(id, &Value::from_string(String::new()))
                        .map_err(|error| Self::redirection_error(location, error))?;
                }
                drop(binding);
                redirect.capture(text)
            }
            else
            {
                File::create(self.eval_path_from(&text))
                    .map(|file| Rc::new(Output::File(file)))
            };
        let result = output.map(|output|
            {
                if stream == 1 || stream == 3 { redirect.output = Some(output.clone()); }
                if stream == 2 || stream == 3 { redirect.error = Some(output); }
            });
        result.map_err(|error| Self::redirection_error(location,
            format!("'{}': {}", target_name, error)))?;
        self.redirections.push(redirect);
        Ok(())
    }

    pub(super) fn finish_redirections(&mut self, depth: usize) -> InterpreterResult<()>
    {
        let mut result = Ok(());
        while self.redirections.len() > depth
        {
            let redirect = self.redirections.pop().unwrap();
            let location = redirect.location.clone();
            let finished = redirect.finish()
                .map_err(|error| Self::redirection_error(&location, error))
                .and_then(|capture|
                    {
                        if let Some((name, text)) = capture
                        { self.write_variable(&location, &name, Value::from_string(text))?; }
                        Ok(())
                    });
            result = result.and(finished);
        }
        result
    }

    pub(super) fn redirect_source(&mut self, location: &Location, value: Value,
                        command_word: bool) -> InterpreterResult<()>
    {
        if    value.is_executable()
           || (command_word && self.can_execute(&value.as_text()))
        {
            return self.execute_value(location, value, Vec::new());
        }
        let Value::String(path, _) = value else
        {
            return Err(Self::redirection_error(location, "A file path must be a string"));
        };
        let path = self.eval_path_from(&path);
        let result = File::open(&path).and_then(|mut file|
            {
                if let Some(output) = self.redirections.iter().rev()
                    .find_map(|redirect| redirect.output.as_ref())
                {
                    output.copy_from(&mut file)
                }
                else if let Some(capture) = self.captured_stdout.as_mut()
                {
                    copy(&mut file, capture)
                }
                else { copy(&mut file, &mut stdout().lock()) }
            });
        result.map_err(|error| Self::redirection_error(location,
            format!("'{}': {}", path, error)))?;
        self.last_result = Some(Value::from_status_code(Some(0)));
        Ok(())
    }

    fn write_stderr(&self, location: &Location, text: &str) -> InterpreterResult<()>
    {
        let result = if let Some(output) = self.redirections.iter().rev()
            .find_map(|redirect| redirect.error.as_ref())
            { output.copy_from(&mut text.as_bytes()).map(|_| ()) }
            else { stderr().lock().write_all(text.as_bytes()) };
        result.map_err(|error| Self::redirection_error(location, error))
    }

    pub(super) fn command_name(location: &Location, value: Value) -> InterpreterResult<String>
    {
        match value
        {
            Value::Type(_) => Err(InterpreterError::new(
                location.clone(),
                ErrorWhat::InvalidOperand("Cannot execute a type as a command".into())
            )),
            Value::Terminal(_) => Err(InterpreterError::new(
                location.clone(),
                ErrorWhat::InvalidOperand(
                    "Cannot execute a terminal as a command".to_string()
                )
            )),
            Value::Struct(_) => Err(InterpreterError::new(
                location.clone(),
                ErrorWhat::InvalidOperand(
                    "Cannot execute a struct as a command".to_string()
                )
            )),
            Value::Enum(_) => Err(InterpreterError::new(
                location.clone(),
                ErrorWhat::InvalidOperand(
                    "Cannot execute an enum as a command".to_string()
                )
            )),
            Value::Range(_) => Err(InterpreterError::new(
                location.clone(),
                ErrorWhat::RangeError("Cannot execute a range as a command".to_string())
            )),
            Value::HashMap(_) => Err(InterpreterError::new(
                location.clone(),
                ErrorWhat::HashMapError(
                    "Cannot execute a hash map as a command".to_string()
                )
            )),
            Value::Array(_) | Value::ArgumentExpansion(_) => Err(InterpreterError::new(
                location.clone(),
                ErrorWhat::ArrayError("Cannot execute an array as a command".to_string())
            )),
            value => Ok(value.as_text())
        }
    }

    fn push(stack: &mut VecDeque<Value>, value: Value)
    {
        stack.push_back(value);
    }

    fn jump_target(instructions: &[Instruction], operand: Option<&Value>,
                    location: &Location) -> InterpreterResult<usize>
    {
        let invalid = || InterpreterError::new(
            location.clone(),
            ErrorWhat::InvalidOperand("Expected a linked jump target".to_string())
        );
        let Some(Value::Integer(target)) = operand else { return Err(invalid()); };
        let target = usize::try_from(*target).map_err(|_| invalid())?;
        if !matches!(instructions.get(target), Some(Instruction { code: Code::JumpTarget, .. }))
        {
            return Err(invalid());
        }
        Ok(target)
    }

    fn range_bound(location: &Location, value: Value) -> InterpreterResult<i64>
    {
        match value
        {
            Value::Integer(value) => Ok(value),
            _ => Err(InterpreterError::new(
                location.clone(),
                ErrorWhat::RangeError("Range bounds must be integers".to_string())
            ))
        }
    }

    // Validate without detaching shared storage. Reads only clone the selected
    // value; writes use Rc::make_mut on each array along the indexed path.
    fn array_index(location: &Location, elements: &[Value],
                    index: &Value) -> InterpreterResult<usize>
    {
        let error = |message| InterpreterError::new(
            location.clone(),
            ErrorWhat::ArrayError(message)
        );
        let Value::Integer(index) = index else
        {
            return Err(error("Array index must be an integer".to_string()));
        };
        let length = elements.len();
        let index = usize::try_from(*index)
            .map_err(
                |_| error(format!("Array index {} is out of bounds for length {}", index,
                                  length)))?;
        if index >= length
        {
            return Err(error(format!(
                "Array index {} is out of bounds for length {}",
                index, length
            )));
        }
        Ok(index)
    }

    fn array_slice(location: &Location, elements: &[Value],
                   range: &Range) -> InterpreterResult<Value>
    {
        let length = elements.len();
        let bound = |value: i64, inclusive: bool| -> InterpreterResult<usize>
            {
                let index = usize::try_from(value).ok().filter(|index|
                    if inclusive { *index < length } else { *index <= length });
                index.ok_or_else(|| InterpreterError::new(
                    location.clone(),
                    ErrorWhat::ArrayError(
                        format!(
                            "Array slice bound {} is out of bounds for length {}",
                            value,
                            length
                        )
                    )
                ))
            };
        let start = range.start.map(|value| bound(value, false)).transpose()?.unwrap_or(0);
        let end = match range.end
            {
                Some(value) => bound(value, range.inclusive)? + usize::from(range.inclusive),
                None => length,
            };
        Ok(Value::from_array(elements[start.min(end)..end].to_vec()))
    }

    fn struct_field_index(location: &Location, item: &StructValue,
                           field: &Value) -> InterpreterResult<usize>
    {
        let TypeKind::Struct(fields) = &item.definition.kind else { unreachable!(); };
        let index = match field
            {
                Value::Integer(index) => usize::try_from(*index)
                    .ok()
                    .filter(|index| *index < fields.len()),
                Value::String(name, _) => fields.iter().position(|field|
                    field.name == name.strip_prefix("::").unwrap_or(name)),
                _ => None
            };
        index.ok_or_else(|| InterpreterError::new(
            location.clone(),
            ErrorWhat::InvalidOperand(
                format!(
                    "Unknown field '{}.{}'",
                    item.definition.name,
                    field.as_text()
                )
            )
        ))
    }

    fn resolve_field_name(&self, location: &Location, field: &Value)
        -> InterpreterResult<Value>
    {
        let Value::String(name, _) = field else { return Ok(field.clone()); };
        let (raw, name) = name.strip_prefix("::")
            .map_or((false, name.as_str()), |name| (true, name));
        if !name.starts_with('$') { return Ok(field.clone()); }
        let value = self.read_raw_variable(name, location)?;
        if matches!(value.underlying(), Value::String(_, _))
        { return Ok(Value::from_string(if raw { format!("::{}", value.as_text()) }
                else { value.as_text() })); }
        Err(InterpreterError::new(
            location.clone(),
            ErrorWhat::InvalidOperand(
                format!(
                    "Field name must be String, got {}",
                    value.type_name()
                )
            )
        ))
    }

    fn resolve_field(
        &mut self,
        location: &Location,
        mut reference: ValueReference,
        value: Value,
        field: &Value,
        snapshot: &Value,
        bind: bool,
    ) -> InterpreterResult<(Value, ValueReference)>
    {
        if !matches!(value.underlying(), Value::Struct(_))
            && !field.as_text().starts_with("::")
            && field.as_text() != "type"
            && let Some(getter) = self.resolve_method(
                &value,
                reference.clone(),
                &format!("get_{}", field.as_text()),
                snapshot,
            )
        {
            let previous = self.last_result.take();
            let result = self.execute_method(location, &getter, &[]);
            let value = self.last_result.take().unwrap_or(Value::None);
            self.last_result = previous;
            result?;
            return Ok((value.clone(), ValueReference::temporary(value)));
        }
        if let Value::Struct(item) = value.underlying()
        {
            let text = field.as_text();
            if crate::language::data::failure::pending(&value).is_some()
                && text != "type" && Self::struct_field_index(location, item, field).is_err()
                && self.resolve_method(&value, reference.clone(), &text, snapshot).is_none()
            { self.consume_success(location, &value)?; }
            let raw = text.starts_with("::");
            let name = text.strip_prefix("::").unwrap_or(&text);
            if !raw
            {
                if let Some(getter) = self.resolve_method(&value, reference.clone(),
                    &format!("get_{}", name), snapshot)
                {
                    let previous = self.last_result.take();
                    let result = self.execute_method(location, &getter, &[]);
                    let value = self.last_result.take().unwrap_or(Value::None);
                    self.last_result = previous;
                    result?;
                    let setter = self.resolve_method(&Value::Struct(item.clone()),
                        reference.clone(), &format!("set_{}", name), snapshot).map(Rc::new);
                    let mut projected = ValueReference::temporary(value.clone());
                    projected.property = Some(Rc::new(PropertyReference
                        {
                            getter: Rc::new(getter), setter, name: name.into(),
                            fresh: true.into(), active: false.into(),
                        }));
                    return Ok((value, projected));
                }
                if self.resolve_method(&value, reference.clone(),
                    &format!("set_{}", name), snapshot).is_some()
                { return Err(protocol_error(location, "Property is write-only")); }
                if self.scope().types.properties.contains_key(&(item.definition.id, name.into()))
                { return Err(protocol_error(location, "Property getter is private")); }
            }
            if let Ok(index) = Self::struct_field_index(location, item, field)
            { self.check_private_field(location, item, index)?; }
        }
        let enum_index =
            matches!(field, Value::String(name, _) if name == "index");
        let result = if matches!(field, Value::String(name, _) if name == "type")
            {
                let result = Value::Type(self.scope().types.get(
                    self.scope().types.value_type(&value)));
                reference = ValueReference::temporary(result.clone());
                result
            }
            else if let Value::Enum(item) = value.underlying() && enum_index
            {
                reference.indexes.push(field.clone());
                reference.fields.push(Value::Boolean(true));
                Value::Integer(item.variant as i64)
            }
            else if let Value::Type(item) = value.underlying()
                && field.as_text() == "name"
            {
                reference.indexes.push(field.clone());
                reference.fields.push(Value::Boolean(true));
                Value::from_string(item.name.clone())
            }
            else if let Value::Struct(item) = value.underlying()
                && let Ok(index) = Self::struct_field_index(location, item, field)
            {
                reference.indexes.push(field.clone());
                reference.fields.push(Value::Boolean(true));
                item.fields[index].clone()
            }
            else if let Some(result) = self.collection_property(
                location, &reference, &value, &field.as_text())
            {
                reference = ValueReference::temporary(result.clone());
                result
            }
            else if let Value::String(name, _) = field
                && let Some(method) = self.resolve_method(
                    &value, reference.clone(), name, snapshot)
            {
                if bind
                {
                    Value::String(format!("{}.{}", value.type_name(), name),
                        Executable::Method(Rc::new(method)))
                }
                else
                {
                    // Receiver and field operations preserve last_result.
                    let previous = self.last_result.take();
                    let result = self.execute_method(location, &method, &[]);
                    let value = self.last_result.take().unwrap_or(Value::None);
                    self.last_result = previous;
                    result?;
                    reference = ValueReference::temporary(value.clone());
                    value
                }
            }
            else
            {
                if let Value::Struct(item) = &value
                { Self::struct_field_index(location, item, field)?; }
                return Err(InterpreterError::new(
                    location.clone(),
                    ErrorWhat::InvalidOperand(
                        format!(
                            "Cannot access a field on {}",
                            value.type_name()
                        )
                    )
                ));
            };
        Ok((result, reference))
    }

    fn check_private_field(&self, location: &Location, item: &StructValue, index: usize)
        -> InterpreterResult<()>
    {
        let TypeKind::Struct(fields) = &item.definition.kind else { unreachable!(); };
        if fields[index].public { return Ok(()); }
        let permitted = self.protocol_context.last().is_some_and(|(owner, origin)|
            *owner == Some(item.definition.id)
                && item.definition.location.as_ref().is_some_and(|location|
                    location.origin.as_ref() == origin));
        if permitted { return Ok(()); }
        Err(protocol_error(location, &format!("Field '{}.{}' is private",
            item.definition.name, fields[index].name)))
    }

    fn get_element(
        location: &Location,
        collection: &Value,
        index: &Value,
    ) -> InterpreterResult<Value>
    {
        match collection.underlying()
        {
            Value::HashMap(values) => Ok(values
                .get(&MapKey::from_value(index))
                .cloned()
                .unwrap_or(Value::None)),
            Value::String(text, _) =>
                {
                    // String helpers in the prelude use Unicode scalar offsets. Do not
                    // materialize one heap-allocated Value per character to implement a slice.
                    let length = text.chars().count();
                    let bound = |value: i64, inclusive: bool| -> InterpreterResult<usize>
                        {
                            usize::try_from(value).ok().filter(|index|
                                if inclusive { *index < length } else { *index <= length })
                                .ok_or_else(|| InterpreterError::new(location.clone(),
                                    ErrorWhat::ArrayError("String index is out of bounds".into())))
                        };
                    match index.underlying()
                    {
                        Value::Integer(index) =>
                            {
                                let index = bound(*index, true)?;
                                Ok(Value::from_string(text.chars().nth(index).unwrap().to_string()))
                            },
                        Value::Range(range) =>
                            {
                                let start = range.start.map(|value| bound(value,
                                    false)).transpose()?.unwrap_or(0);
                                let end = range.end.map(|value| bound(value, range.inclusive)
                                    .map(|value| value +
                                        usize::from(range.inclusive))).transpose()?.unwrap_or(length);
                                let start =
                                    text.char_indices().nth(start.min(end)).map_or(text.len(),
                                    |(offset, _)| offset);
                                let end = text.char_indices().nth(end).map_or(text.len(),
                                    |(offset, _)| offset);
                                Ok(Value::from_string(text[start..end].to_string()))
                            },
                        _ => Err(InterpreterError::new(location.clone(),
                            ErrorWhat::ArrayError("String index must be an Integer or range".into()))),
                    }
                },
            Value::Array(values) => match index
                {
                    Value::Range(range) => Self::array_slice(location, values, range).map(|value|
                        if let Some(id) = values.type_id { value.with_collection_type(id) }
                        else { value }),
                    _ => Ok(values[Self::array_index(location, values, index)?].clone()),
                },
            _ => Err(InterpreterError::new(
                location.clone(),
                ErrorWhat::ArrayError(
                    "Cannot index a value that is not an array or hash map".to_string()
                )
            ))
        }
    }

    fn set_element(
        &mut self,
        location: &Location,
        collection: &mut Value,
        indexes: &[Value],
        fields: &[Value],
        value: Value,
    ) -> InterpreterResult<()>
    {
        let Some((index, rest)) = indexes.split_first() else
        {
            *collection = value;
            return Ok(());
        };
        if let Value::Named(item) = collection
        {
            let (indexes, fields) = if matches!(fields.first(), Some(Value::None))
                { (rest, &fields[1..]) } else { (indexes, fields) };
            return self.set_element(location, &mut Rc::make_mut(item).value,
                                      indexes, fields, value);
        }
        if matches!(fields.first(), Some(Value::Boolean(true)))
            && matches!(collection, Value::Struct(_))
        {
            let name = index.as_text();
            if !name.starts_with("::")
            {
                let reference = ValueReference::temporary(collection.clone());
                let getter = self.resolve_method(collection, reference.clone(),
                    &format!("get_{}", name), &Value::None);
                let setter = self.resolve_method(collection, reference.clone(),
                    &format!("set_{}", name), &Value::None);
                if getter.is_some() || setter.is_some()
                {
                    let setter = setter.ok_or_else(||
                        protocol_error(location, "Property is read-only"))?;
                    let mut child = if rest.is_empty() { value.clone() }
                        else
                        {
                            let getter = getter.ok_or_else(||
                                protocol_error(location, "Property is write-only"))?;
                            let previous = self.last_result.take();
                            let result = self.execute_method(location, &getter, &[]);
                            let child = self.last_result.take().unwrap_or(Value::None);
                            self.last_result = previous;
                            result?;
                            child
                        };
                    if !rest.is_empty()
                    { self.set_element(location, &mut child, rest, &fields[1..], value)?; }
                    let previous = self.last_result.take();
                    let result = self.execute_method(location, &setter, &[child]);
                    self.last_result = previous;
                    result?;
                    *collection = reference.root.borrow().value.clone();
                    return Ok(());
                }
                if let Value::Struct(item) = collection
                    && self.scope().types.properties.contains_key(&(item.definition.id, name))
                { return Err(protocol_error(location, "Property setter is private")); }
            }
        }
        if matches!(fields.first(), Some(Value::Boolean(true)))
        {
            if matches!(index, Value::String(name, _) if name == "type")
            {
                return Err(InterpreterError::new(
                    location.clone(),
                    ErrorWhat::InvalidOperand("The .type property is read-only".into())
                ));
            }
            if    matches!(collection, Value::Type(_))
               && matches!(index, Value::String(name, _) if name == "name")
            {
                return Err(InterpreterError::new(
                    location.clone(),
                    ErrorWhat::InvalidOperand("Type name is read-only".into())
                ));
            }
            if    matches!(collection, Value::Array(_) | Value::HashMap(_))
               && matches!(index, Value::String(name, _) if name == "value_type"
                    || (name == "key_type" && matches!(collection, Value::HashMap(_))))
            {
                return Err(InterpreterError::new(
                    location.clone(),
                    ErrorWhat::InvalidOperand(
                        "Collection type properties are read-only".into()
                    )
                ));
            }
            if    matches!(collection, Value::Enum(_))
               && matches!(index, Value::String(name, _) if name == "index")
            {
                return Err(InterpreterError::new(
                    location.clone(),
                    ErrorWhat::InvalidOperand("Enum index is read-only".to_string())
                ));
            }
            let Value::Struct(item) = collection else
            {
                return Err(InterpreterError::new(
                    location.clone(),
                    ErrorWhat::InvalidOperand(
                        format!(
                            "Cannot assign a field on {}",
                            collection.type_name()
                        )
                    )
                ));
            };
            if item.definition.location.as_ref().is_some_and(|location|
                location.origin.as_ref() == "<native>") && matches!(item.definition.name.as_str(),
                    "SourceLocation" | "CallFrame" | "CallStack" | "ErrorSnapshot" |
                        "RelatedFailure")
            { return Err(protocol_error(location, "Diagnostic snapshots are immutable")); }
            let index = Self::struct_field_index(location, item, index)?;
            self.check_private_field(location, item, index)?;
            let TypeKind::Struct(definitions) = &item.definition.kind else { unreachable!(); };
            let value = if rest.is_empty()
                { self.scope().types.coerce(definitions[index].type_id, value)
                    .map_err(|message| protocol_error(location, &format!(
                        "Type error: Field '{}.{}' (declared at {}): {}",
                        item.definition.name, definitions[index].name,
                        definitions[index].location, message)))? } else { value };
            return self.set_element(
                location,
                &mut Rc::make_mut(item).fields[index],
                rest,
                &fields[1..],
                value,
            );
        }
        match collection
        {
            Value::HashMap(values) =>
                {
                    let key = MapKey::from_value(index);
                    if rest.is_empty()
                    {
                        Rc::make_mut(values).insert(key, value);
                        return Ok(());
                    }
                    // Missing intermediate keys do not implicitly create containers.
                    if !values.contains_key(&key)
                    {
                        return Err(InterpreterError::new(
                            location.clone(),
                            ErrorWhat::HashMapError(
                                "Missing intermediate key in indexed assignment".to_string()
                            )
                        ));
                    }
                    let child = Rc::make_mut(values).get_mut(&key).unwrap();
                    self.set_element(location, child, rest, &fields[1..], value)
                },
            Value::Array(values) =>
                {
                    if matches!(index, Value::Range(_))
                    {
                        return Err(InterpreterError::new(
                            location.clone(),
                            ErrorWhat::ArrayError(
                                "Array slices are read-only".to_string()
                            )
                        ));
                    }
                    let index = Self::array_index(location, values, index)?;
                    self.set_element(
                        location,
                        &mut Rc::make_mut(values)[index],
                        rest,
                        &fields[1..],
                        value,
                    )
                },
            _ => Err(InterpreterError::new(
                location.clone(),
                ErrorWhat::ArrayError(
                    "Cannot index a value that is not an array or hash map".to_string()
                )
            ))
        }
    }

    fn pop(location: &Location, stack: &mut VecDeque<Value>) -> InterpreterResult<Value>
    {
        stack.pop_back().ok_or_else(||
            InterpreterError::new(location.clone(), ErrorWhat::StackUnderflow))
    }

    fn pop_as_text(location: &Location,
               stack: &mut VecDeque<Value>) -> InterpreterResult<String>
    {
        Ok(Self::pop(location, stack)?.as_text())
    }

    fn handle_string_interpolation(
        &mut self, location: &Location, stack: &mut VecDeque<Value>, escaped_dollars: &[usize],
        escape_glob: bool,
    ) -> InterpreterResult<()>
    {
        let invalid_operand = |message| InterpreterError::new(
            location.clone(),
            ErrorWhat::InvalidOperand(message)
        );

        let Value::String(text, executable) = Self::pop(location, stack)? else
        {
            return Err(invalid_operand(
                "Expected a string for InterpolateString instruction.".to_string()));
        };

        let interpolated = self.interpolate_string_at(
            location,
            &text,
            escaped_dollars,
            !escape_glob,
            escape_glob,
        )?;
        Self::push(stack, Value::String(interpolated, executable));
        Ok(())
    }

    pub(super) fn interpolate_string(&mut self, location: &Location, text: &str)
        -> InterpreterResult<String>
    {
        self.interpolate_string_at(location, text, &[], true, false)
    }

    fn interpolate_string_at(&mut self, location: &Location, text: &str,
                             escaped_dollars: &[usize], display_paths: bool,
                             escape_glob: bool) -> InterpreterResult<String>
    {
        let invalid_operand = |message| InterpreterError::new(
            location.clone(),
            ErrorWhat::InvalidOperand(message)
        );

        let mut interpolated = String::with_capacity(text.len());
        let mut offset = 0;
        for reference in interpolation_references(text, escaped_dollars)
        {
            interpolated.push_str(&text[offset..reference.span.start]);
            if let Some(message) = reference.error
            { return Err(invalid_operand(message.to_string())); }
            let mut parts = reference.name.split('.');
            let name = parts.next().unwrap();
            let mut value = self.read_raw_variable(name, location)?;
            let mut receiver = self.variable_reference(name)
                .unwrap_or_else(|| ValueReference::temporary(value.clone()));
            for name in parts
            {
                let field = self.resolve_field_name(location, &Value::from_string(name.into()))?;
                (value, receiver) = self.resolve_field(
                    location, receiver, self.eval_value_paths_to(value), &field,
                    &Value::None, false)?;
            }
            let value = if display_paths { self.eval_value_paths_to(value) } else { value };

            // Append values directly so their contents are not interpolated again.
            let text = value.as_text();
            let text = if escape_glob { self.eval_path_from(&text) } else { text };
            interpolated.push_str(&if escape_glob { Pattern::escape(&text) } else { text });
            offset = reference.span.end;
        }
        interpolated.push_str(&text[offset..]);

        Ok(interpolated)
    }

    /**
     * Resolve the current shell's home directory without interpolating its contents.
     */
    fn home_path(&self) -> Option<String>
    {
        self.scope().variables.get("$HOME")
            .map(|home| home.value.as_text())
            .filter(|home| !home.is_empty())
            .or_else(|| home_dir().map(|home| home.to_string_lossy().into_owned()))
            .filter(|home| Path::new(home).is_absolute())
    }

    /**
     * Shorten a leading home directory for display, matching complete path segments.
     */
    pub(super) fn eval_path_to(&self, path: &str) -> String
    {
        let Some(home) = self.home_path() else { return path.to_string(); };
        let home = home.trim_end_matches(is_separator);

        // A root home must not become an empty prefix that matches relative paths.
        if home.is_empty()
        {
            return if path == MAIN_SEPARATOR_STR
                {
                    "~".to_string()
                }
                else if path.starts_with(is_separator)
                {
                    format!("~{}", path)
                }
                else
                {
                    path.to_string()
                };
        }

        if path == home
        {
            return "~".to_string();
        }

        if    let Some(suffix) = path.strip_prefix(home)
           && suffix.starts_with(is_separator)
        {
            return format!("~{}", suffix);
        }

        path.to_string()
    }

    /**
     * Expand only the current user's leading ~ or ~/ prefix. Callers decide
     * whether the source word is eligible; quoted words and variables are literal.
     */
    pub(super) fn eval_path_from(&self, path: &str) -> String
    {
        let Some(suffix) = path.strip_prefix('~') else { return path.to_string(); };
        if !suffix.is_empty() && !suffix.starts_with(is_separator)
        {
            return path.to_string();
        }

        let Some(home) = self.home_path() else { return path.to_string(); };
        if suffix.is_empty()
        {
            return home;
        }

        format!("{}{}", home.trim_end_matches(is_separator), suffix)
    }

    /**
     * PATH entries must be real paths when searching for or launching programs.
     */
    fn eval_path_list_from(&self, paths: &str) -> String
    {
        let expanded = split_paths(paths)
            .map(|path| PathBuf::from(self.eval_path_from(&path.to_string_lossy())));

        join_paths(expanded)
            .map(|paths| paths.to_string_lossy().into_owned())
            .unwrap_or_else(|_| paths.to_string())
    }

    fn handle_file_glob(&self, pattern: &str, expand_tilde: bool) -> InterpreterResult<Value>
    {
        // Glob results may omit an explicit "./" prefix or normalize separators.
        // Normalize both sides for matching without resolving parent directories.
        fn normalized_path(path: &Path) -> PathBuf
        {
            path.components()
                .filter(|component| !matches!(component, Component::CurDir))
                .collect()
        }

        let display_pattern = self.eval_path_to(pattern);
        let expanded = if expand_tilde
        {
            self.eval_path_from(pattern)
        }
        else
        {
            pattern.to_string()
        };
        let pattern = if expanded != pattern
            {
                let suffix = pattern.strip_prefix('~').unwrap();
                let home = expanded.strip_suffix(suffix).unwrap();
                format!("{}{}", Pattern::escape(home), suffix)
            }
            else
            {
                expanded
            };

        let options = MatchOptions
            {
                require_literal_separator: true,
                require_literal_leading_dot: true,
                ..MatchOptions::new()
            };

        let invalid_pattern = |error| InterpreterError::new(
            Location::default(),
            ErrorWhat::InvalidOperand(
                format!("Invalid glob pattern '{}': {}", display_pattern, error)
            )
        );

        let matcher = Pattern::new(
            &normalized_path(Path::new(&pattern)).to_string_lossy())
            .map_err(&invalid_pattern)?;

        // glob_with's leading-dot option prunes even explicitly requested hidden
        // entries. Enumerate normally, then enforce that rule with Pattern instead.
        let paths = glob(&pattern).map_err(invalid_pattern)?;

        let mut arguments = Vec::new();

        for entry in paths
        {
            let path = entry.map_err(|error| InterpreterError::new(
                Location::default(),
                ErrorWhat::FileGlobError(
                    format!("Failed to expand '{}': {}", display_pattern, error)
                )
            ))?;

            let path_text = path.to_string_lossy();

            // Inspect the final entry as written: Path::file_name normalizes
            // away a trailing "/.", which would hide that special entry.
            let entry_name = path_text.trim_end_matches(is_separator)
                .rsplit(is_separator).next();

            if matches!(entry_name, Some(".") | Some(".."))
            {
                continue;
            }

            if !matcher.matches_path_with(&normalized_path(&path), options)
            {
                continue;
            }

            arguments.push(Value::from_string(path_text.into_owned()));
        }

        if arguments.is_empty()
        {
            return Err(InterpreterError::new(
                Location::default(),
                ErrorWhat::FileGlobError(
                    format!("No paths matched glob pattern '{}'.", display_pattern)
                )
            ));
        }

        Ok(Value::from_argument_expansion(arguments))
    }

    pub(super) fn resolve_alias(&self,
                     location: &Location,
                     command: &str) -> InterpreterResult<(String, Vec<String>, String)>
    {
        let mut name = command.to_string();
        let mut home = self.current_scope.clone();
        let mut arguments = VecDeque::new();
        let mut visited = HashSet::new();

        while let Some(alias) = self.module_alias_in(&home, &name)
        {
            if !visited.insert((home.clone(), name.clone()))
            {
                return Err(InterpreterError::new(
                    location.clone(),
                    ErrorWhat::InvalidOperand(
                        format!(
                            "Alias cycle detected at '{}' while resolving '{}'.",
                            name,
                            command
                        )
                    )
                ));
            }

            // Target arguments precede the arguments of aliases that refer to it.
            for argument in alias.arguments.iter().rev()
            {
                arguments.push_front(argument.clone());
            }

            // An alias such as `ls = ls -h` adds defaults to the real command.
            // Apply it once, including when reached through another alias.
            let same_command = alias.name == name.rsplit("::").next().unwrap();
            home = alias.scope.clone();
            name = alias.name.clone();
            if same_command
            {
                break;
            }
        }

        Ok((name, arguments.into_iter().collect(), home))
    }

    fn handle_cd(&mut self, location: &Location, args: &[Value]) -> InterpreterResult<()>
    {
        let args = args.iter().map(Value::as_text).collect::<Vec<_>>();
        let args = args.as_slice();
        if args.len() != 1
        {
            self.write_stderr(location, "Usage: cd <directory>\n")?;
            self.last_result = Some(Value::ExecResult(ExecResult::Value(1)));
            return Ok(());
        }

        if let Err(error) = set_current_dir(self.eval_path_from(&args[0]))
        {
            self.write_stderr(location, &format!("Failed to change directory to {}: {}\n",
                self.eval_path_to(&args[0]), error))?;
            self.last_result = Some(Value::ExecResult(ExecResult::Value(1)));
            return Ok(());
        }

        self.last_result = Some(Value::ExecResult(ExecResult::Value(0)));
        Ok(())
    }

    fn handle_exit(&mut self, location: &Location, args: &[Value]) -> InterpreterResult<()>
    {
        if self.exceptions.cleanup.is_some()
        {
            return Err(InterpreterError::new(
                location.clone(),
                ErrorWhat::CleanupControl,
            ));
        }
        let args = args.iter().map(Value::as_text).collect::<Vec<_>>();
        let args = args.as_slice();
        self.exit_code = match args
            {
                [] => 0,
                [code] => code.parse::<u8>().map_err(|_| InterpreterError::new(
                    location.clone(),
                    ErrorWhat::ArgumentMismatch(
                        "exit expects a status from 0 to 255.".to_string()
                    )
                ))?,
                _ => return Err(InterpreterError::new(
                    location.clone(),
                    ErrorWhat::ArgumentMismatch(
                        "exit expects at most one argument.".to_string()
                    )
                ))
            };
        self.halted = true;
        self.last_result = Some(Value::None);

        Ok(())
    }
}


#[track_caller]
fn protocol_error(location: &Location, message: &str) -> InterpreterError
{
    InterpreterError::new(location.clone(), ErrorWhat::InvalidOperand(message.into()))
}

fn seal_constructed(value: &mut Value)
{
    match value
    {
        Value::Struct(item) =>
            {
                let item = Rc::make_mut(item);
                item.constructing = false;
                for value in &mut item.fields { seal_constructed(value); }
            },
        Value::Named(item) => seal_constructed(&mut Rc::make_mut(item).value),
        Value::Array(values) | Value::ArgumentExpansion(values) =>
            for value in Rc::make_mut(values) { seal_constructed(value); },
        Value::HashMap(values) =>
            for value in Rc::make_mut(values).values_mut() { seal_constructed(value); },
        _ => {},
    }
}
