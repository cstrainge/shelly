# Networking and JSON API design

Status: core implemented and being validated, 2026-10-13. The executable library surface,
initial transport profiles and compatibility changes are recorded in
[the implementation reference](networking-json-api-implementation.md). Schema, Patch,
Merge Patch, canonicalization and numeric comparison remain the later utility layer.

OpenAI is an ordinary application sample in [examples/openai.shy](../examples/openai.shy).
It is **not a standard-library module**. Multi-file Shelly modules use directory/init.shy:
networking starts at [std/net/init.shy](../std/net/init.shy), and HTTP starts at
[std/net/http/init.shy](../std/net/http/init.shy). Neither directory has a conflicting sibling
net.shy or http.shy.

Shelly should support sockets, HTTP, REST, and JSON-RPC through consistent typed values,
explicit byte streams, and libraries that can be extended in Shelly. JSON supplies one
conversion model across direct calls, pipeline boundaries, network bodies, and application
types. Marked error structs, named error unions, exceptions, and call stacks supply the shared
failure model.

This revision resolves the five blocking contracts from the [first hostile review](networking-json-api-review.md).
The [second hostile review](networking-json-api-review-2.md) records the counterexamples tried
against the revised design and corrections made. The contracts below describe the complete
target API, with the initial profiles and executed behavioral coverage in the implementation reference.

The usability criterion is simple: ordinary calls return the useful value or enter a catch.
JSON conversion protocols handle representation, named unions organize errors, and operation
leases handle pipeline cleanup. A workflow should contain substantially more useful work than
error-handling code. No checked-exception boilerplate is introduced.

## Existing foundations and required changes

The current language already supports nominal named types and unions, module privacy,
constructors, conversion factories, mutable instance receivers, closures, and source iterators.
Only the source of a pipeline expands automatically; arrays returned by later stages remain
single items. Bare $ denotes the current pipeline item.

Native process pipelines preserve bytes. Unannotated bytes entering ordinary scripted stages
currently become a String after EOF. The JSON boundary currently buffers a complete document.
The next_item protocol reserves bare () for end of iteration. Custom iteration uses a private
receiver copy, so ordinary field mutations are not a shared stream lifecycle mechanism.

Runtime diagnostics now contain the original Shelly failure location, optional Rust failure
origin, and CallFrame records ordered innermost first. Frames include a call name, Shelly call
location, native flag, and optional native dispatch boundary. This trace is accumulated as
interpreter errors unwind. Script-visible CallStack, normal-return failure capture, marked
errors, and try/catch/finally now build on that trace.

Relevant foundations:

- [Pipeline behavior](../README.md#pipelines).
- [Conversion and structural mapping](../src/language/data/formats/mapping.rs).
- [Current JSON engine](../src/language/data/formats/json/mod.rs).
- [Iteration](../src/language/interpreter/iteration.rs).
- [Call frames](../src/language/interpreter/call_stack.rs).
- [Runtime diagnostics](../src/language/interpreter/interpreter.rs).
- [Call-stack behavioral coverage](../tests/native/call_stacks.shy).

The previous JSON encoder included private struct storage and its decoder used native
numeric types. ABI 1 replaces those defaults with public structural mapping and exact
documents; ordinary materialization explicitly checks native numeric representability.

## Library boundaries

| Layer | Responsibility |
| --- | --- |
| std::io | Bytes, readers, writers, deadlines, transfer and resource leases |
| std::stream | Single-consumer scripted item readers and completion results |
| std::json | Parsing, encoding, exact documents, conversion, framing and validation |
| std::net::dns, tcp, udp, unix, tls | Addresses, sockets, listeners and secure transports |
| std::net::http | HTTP clients, requests, responses, body transport and servers |
| std::net::sse | Server-Sent Events framing over byte readers |
| std::net::rpc | JSON-RPC envelopes, correlation, transports and dispatch |
| Application modules | Endpoint models, authentication choices and service-specific policy |
| Prelude and interpreter | Error obligations, exceptions, CallStack and report_error |

JSON depends on shared I/O, not networking. HTTP and RPC consume the JSON interface.
Rust owns operating-system resources, readiness, TLS and established HTTP protocol engines.
Shelly owns public wrappers, codec policy, framing composition, pagination, RPC envelopes,
routing and application models.

Register implementation primitives as hidden and expose them locally inside their wrappers.
Existing visible declarations can reveal hidden natives from other modules: this is interface
encapsulation, not exclusive access. True owner-only native access requires a separate
module-provenance permission design.

## Marked errors and exception handling

### Error values and named groups

An error struct is an ordinary nominal struct with a failure marker. A **payload** is typed
data; a **failure occurrence** is one activation of that payload, with an origin snapshot and
responsibility token. Caught payloads can be stored as history without becoming new failures.

~~~shy
pub error struct ApiError
{
    pub status: Integer,
    pub message: String,
    pub request_id: optional String,
    pub response: optional std::json::Document,
}

pub type Error = ApiError | std::net::http::Error | std::json::Error | std::stream::Error

pub fn Client::create($request: Request): Response | Error
{
    # Return a response or a marked failure.
}
~~~

Groups organize signatures and catches; recognition follows named wrappers to concrete marked
types. They are not checked exception effects: callbacks can throw other types, and callers
need neither exhaustive catches nor a growing list of checks. One untyped workflow catch is
valid. Ordinary nominal success types retain their existing behavior.

The prelude also supplies ErrorValue, an abstract constraint accepting any concrete marked
payload without wrapping it. It cannot be constructed or structurally decoded; it supports
.type, narrowing and diagnostic inspection, not an assumed common message field. An explicit
ErrorValue scalar parameter/binding is error-inclusive capture. An untyped catch uses this
open constraint, so new library errors do not require changing the prelude signature.

<a id="unhandled-failure-obligations"></a>

### Success consumption and explicit capture

The ordinary path is deliberately short:

~~~shy
try
{
    let $response = $client.get "users/42"
    let $user = $response.require_success.read_as User
    save $user
}
catch ($stack: CallStack, $error)
{
    report_error $stack $error
}
~~~

An inferred binding consumes success. It does not retain a success/error union. Passing to an
any parameter also consumes success: any is not an error-suppression escape hatch.

| Context | Failure behavior |
| --- | --- |
| Inferred binding, success-only annotation or any | Promote an active marked failure before coercion; otherwise validate success normally |
| Ordinary argument, success-only receiver, collection item/key or struct field | Consume success before calling or mutating storage |
| Explicit scalar annotation containing marked alternatives | Capture a compatible pending occurrence |
| Function return admitting that marked alternative | Transfer the root occurrence to the caller |
| Return excluding that error, ordinary pipeline boundary or iteration | Promote before incompatible-success checking or emitting data |
| Bare call, unused temporary or explicit _ discard | Promote an active failure |
| Catch | Acknowledge the entire incoming diagnostic packet before entering the handler |
| Inspection/display of an explicitly captured occurrence | Observe without acknowledging |
| throw, or returning a caught marked payload as the root failure | Propagate/reactivate with original provenance |

~~~shy
let $reply = fetch                    # Reply, or an exception.
let $reply: Reply = fetch             # Same behavior, with a success check.
let $saved: Reply | Error = fetch     # Explicitly captures a pending scalar.
~~~

Only an explicit top-level error-inclusive scalar type captures. An optional error type
qualifies; [Reply | Error] does not. Pending occurrences cannot enter containers or captured
closure cells: insertion promotes before mutation. This avoids tracking responsibility over
arbitrary heap graphs. After catch, immutable diagnostic data can be stored and shared freely:

~~~shy
pub struct Failure
{
    pub stack: CallStack,
    pub error: any,
}

try { operation }
catch ($stack: CallStack, $error)
{
    $failures = [$failures..., Failure(stack: $stack, error: $error)]
}
~~~

Copying passive history does not reactivate it. Explicit throw, root failure return or explicit
conversion into a marked target does. Reading a Failure.error field remains passive. Provenance
is runtime metadata, separate from the user's struct fields.

### Responsibility state machine

Each pending occurrence has a monotonically increasing activation ID, one owning lexical
region, and tracked scalar slots. Regions are function activations, lexical blocks, protected
try bodies and the root submission. A producer's root return transfers into the receiving
region. VM instructions maintain the ledger; Rust Drop and Rc counts never trigger failures.

| Event | Ledger transition |
| --- | --- |
| Construct marked root payload | Create Pending(id, owner, provenance) after successful construction |
| Capture/copy into an allowed local scalar | Add a slot to the same ID; no new occurrence |
| Pass to an explicit error-inclusive parameter | Borrow; responsibility remains with the owner |
| Overwrite a slot | Remove its old reference; if it is the last owned slot, promote before writing |
| Discard or success consumption | Remove occurrence from ledger and throw, even if aliases remain |
| Owner exit | Throw outstanding occurrences owned by that region |
| Compatible root return | Put occurrence in return escrow; transfer only after cleanup succeeds |
| Catch | Acknowledge every occurrence in packet; aliases become passive history |
| Return/throw acknowledged history | Fresh activation retaining original provenance |
| Commit marked error conversion | Retire active source tokens into passive history; outcome owns replacement obligation |

Slots include locals, expression temporaries and borrowed scalar arguments. The VM applies
these transitions at statement, assignment, call and region boundaries without scanning objects.
Copies within the owner or descendants are allowed; writes into ancestor/global/module bindings
or closure-captured bindings promote before mutation. Capturing a binding already holding a
pending occurrence also promotes; later writes to captured cells use that same rule.
Fresh root-submission locals are owned scalar slots even when the REPL preserves their names.
The global-write restriction concerns bindings outside that ownership region, not a fresh
top-level explicit capture. Root catch/exit resolves it before the next submission.

A descendant can observe/borrow a parent's token but cannot transfer its responsibility by
returning it. Its return passes the same borrowed token back; only the owner's eventual root
return transfers ownership. The last owned slot's overwrite promotes even if borrowed aliases
survive. Incidental aliases cannot postpone owner exit.

On owner exit, earliest activation is primary; other tokens attach in activation order as
Unhandled relations. Remove all from ledgers before dispatch. A protected region's catch is
still active during this check. If an exception is already propagating, it stays primary and
outstanding tokens attach instead of replacing it.

A compatible return excludes its escrowed token from owner-exit checks. Local aliases expire
without duplicate promotion. Cleanup failure during that return makes the escrowed failure
primary with cleanup attached. An incompatible return consumes success and throws locally;
nominal wrapping cannot disguise it. No pending token survives a REPL submission.

A field/method whose receiver is explicitly a marked type can inspect a captured occurrence;
.type is observational as well. Access demanding a successful Reply receiver promotes.
Branch/block expression results inherit the receiving context; a root value crosses that
expression region in escrow instead of being discarded at its closing brace.

Raw storage inside a marked type's factory is not activated early. Activation occurs only after
the factory returns a validated marked payload; a factory that fails instead creates that failure.

If owner exit abandons a returned pending occurrence because another token is outstanding,
the escrowed failure is primary and the other tokens attach as Unhandled. If a successful return
is abandoned, earliest outstanding activation is primary. This closes the return-plus-alias case.

These restrictions concern explicit capture. Ordinary network/JSON code requires no pending
locals, acknowledgement calls or boilerplate checks.

### Typechecking and conversion contexts

The compiler and interpreter share four contexts:

| Context | Rule |
| --- | --- |
| ConsumeSuccess | Remove only active marked alternatives before checking expected success; wrong successful types still fail |
| Capture | Preserve compatible error-inclusive scalars and ordinary nominal assignment rules |
| Propagate | Admit declared root errors; preserve concrete payload/occurrence through named return groups |
| CatchFilter | Flatten aliases/groups to concrete marked TypeIDs; never wrap the payload in a nominal group |

An active error incompatible with a requested capture type promotes the original failure;
it does not become an ordinary type-mismatch error. Capture cannot silence an unexpected error.

Catch-filter bindings have the flattened concrete union as their static type. .type is the
concrete type and existing narrowing applies. Clause compilation pins TypeIDs, including across
REPL redefinitions; matching never uses spelling. Ordinary named-group bindings still use
nominal wrapping. This is not a global change to conversion.

ConsumeSuccess projects a callable's mixed success/error result: named wrappers whose
underlying alternatives include marked errors are transparent in that projection. Pure success
nominal types retain their identity. A call returning Result = Reply | Error therefore binds
a successful Reply as Reply; an explicit Result annotation captures/wraps instead. Copying an
already caught passive error is not a fallible call and retains its concrete type. Promotion
depends on active occurrence metadata, never merely on a stored type being marked.

Factories may return their own type (or optional form) plus marked alternatives. Wrong successful
types fail at the factory boundary. Raw construction inside the factory retains existing access
rules. JSON hooks require Document, the target type or Codec as their successful result,
respectively. Native metadata must express the same contracts.

A fallible callback fits a success-only prototype if all non-error alternatives fit; an execution
adapter promotes returned errors. A dynamic any result is checked when it arrives. This does
not claim the callback cannot throw.

### Conversions should remove glue code

Use existing from_<source> factories rather than parallel network conversion callbacks.
A Document uses from_json; ordinary hashes retain key/value and nominal identities for
source-specific selection. json::convert, HTTP read_as and typed JSON pipeline boundaries share
dispatch and strict fallback. Conversion failures enter the surrounding catch automatically.

Error conversion is **explicit**: catch matching never executes conversion code.

~~~shy
pub error struct ServiceError
{
    pub message: String,
}

pub fn ServiceError::from_statuserror($error: std::net::http::StatusError): ServiceError
{
    ServiceError(message: "Service request failed: ${error.status}")
}

try { fetch_profile }
catch ($error: std::net::http::StatusError)
{
    throw ServiceError($error)
}
~~~

The normal conversion call selects from_statuserror. The converted payload is a new occurrence
with the source attached as Cause automatically; no stack copying or manual cause field is
required. This follows marked source-argument provenance even after catch, including explicit
conversion outside that handler. Multiple marked arguments attach causes in parameter order;
conversion failure retains those sources as causes. Deduplicate by occurrence identity.
Ordinary diagnostic containers do not implicitly cause error conversion.

When an explicit conversion to a marked target commits its returned/escaping failure, replace
any active source obligations with that outcome's token/packet atomically. Source aliases become
passive history; the new outcome owns the obligation and retains source causes. Do not leave
both the old error and its domain replacement independently pending. Before commitment, source
borrows remain unchanged. A failed conversion performs the same replacement with its escaping
failure. Conversion to a non-error target, such as String for display, only observes the source.

Usually a union is sufficient: export ServiceError = DomainError | http::Error | json::Error
when retaining concrete lower-level failures helps callers. Convert when domain meaning changes.

### Try and catch

~~~shy
try
{
    let $response = operation
    consume $response
}
catch ($stack: CallStack, $error: ApiError)
{
    report_error $stack $error
}
catch (_, $error: std::json::Error)
{
    echo $error.message
}
catch ($error: net::Error)
{
    echo $error.message
}
finally
{
    $resource.close
}
~~~

All catches receive a CallStack and concrete error. The shorter forms omit stack bindings;
an untyped error binding catches all exceptions. Match clauses in source order. Groups match
only actual marked exceptions even if their declarations contain non-error alternatives.

A handler acknowledges the packet before executing. A new handler failure attaches the caught
packet as Handling context unless already a cause. Sibling clauses do not catch handler/finally
failures. Rethrow of the same error preserves origin and adds propagation without a self-cause.

try initially is a statement. A compatible explicit return transfers through finally without
entering its local catch; throw enters catch immediately. Parse/compile failures are reported
at the submission boundary and cannot be caught inside the rejected submission.

### Finally and cleanup

Each region leaves with one completion record: Normal, Return(value/escrow), Break(target),
Continue(target), Throw(packet) or Exit(status). Finally runs after the selected catch and
before that record leaves the try. Pipeline/reader unwinding releases operation leases before
the enclosing catch; user finally handles caller-owned persistent resources.

| Original completion | Cleanup | Result |
| --- | --- | --- |
| Normal, successful return or loop exit | Success | Resume original completion |
| Normal or successful return | Failure | Cleanup primary; cancel successful return |
| Throw or returned pending failure in escrow | Failure | Original primary; attach cleanup with its own stack |
| Any | Multiple failures | Preserve first applicable primary; attach others in cleanup order |

Automatic releases run in reverse acquisition order, at most once each including failed release.
Finally is entered at most once and is not restarted after interruption. Output, mutations and
remote effects are not rolled back.

A cleanup guard forbids return/break/continue/exit escaping that guard, including exit inside
a helper. Helpers may return normally and use local loops. Direct escaping syntax fails
compilation; indirect escaping exit raises CleanupControl. Throw is allowed.

Normal and exceptional cleanup share a cumulative **5,000 ms / 1,000,000 VM instruction**
budget per leaving operation, whichever expires first. The outermost unwind/operation-release
creates the budget; nested finally, helper calls and related releases inherit it. Reaching a
catch ends that unwind. Its handler is ordinary code; later normal finally creates a cleanup
budget if no parent cleanup is active. Nested cleanup cannot reset an active budget.
The initial release fixes these budgets; configurable larger normal budgets are deferred.
Native waits share the remaining deadline; back-edge/instruction checks stop scripted loops.
Budget exhaustion skips remaining script cleanup, attaches CleanupTimeout and performs mandatory
native releases. A previously successful completion then becomes a cleanup failure.

First Ctrl+C cancels waits and starts Interrupted unwinding. Cleanup masks only that consumed
cancellation generation, allowing local release without resetting an expired network deadline.
A new Ctrl+C during this unwind produces CleanupAbort, bypasses script catches until root,
skips remaining script cleanup and runs native releases. Original interruption stays primary;
the REPL reports and recovers. First Ctrl+C during otherwise normal finally cancels that
finally; remaining outer cleanup runs in interrupted mode.

close is local release, not TLS close-notify, TCP drain or remote acknowledgement. Graceful
shutdown is a separate ordinary deadline-bound operation. Native releases must not wait for
peers or execute script. Workers dispose of late completions without interpreter access.
Cleanup cannot be guaranteed after SIGKILL or a nonreturning kernel operation; supported waits
must not run uncancellable blocking work on the interpreter thread.

### Call stacks and reporting

Expose immutable CallStack snapshots:

| Field | Meaning |
| --- | --- |
| error_type | Concrete TypeID associated with this diagnostic |
| origin | Original Shelly failure location |
| native_origin | Optional Rust failure location |
| frames | Innermost-first CallFrames, including distinct native_boundary sites |
| propagation | Later throw/conversion sites without rewriting origin frames |
| related | Ordered RelatedFailure records |
| truncated | Whether bounded capture omitted information |

RelatedFailure contains relation (Cause, Cleanup, Unhandled, Handling or Reporting),
error: ErrorSnapshot, and stack: CallStack. ErrorSnapshot exposes original_type: Type,
public_fields: bounded DiagnosticValues, optional formatted preview, and truncated. DiagnosticValue
represents scalars, container previews and opaque resource identities. It is explicitly a
projection, not an invalid partially copied instance of the original struct. Private fields are
not exposed. A snapshot never owns a socket or retains an arbitrarily large source document.

The handler's primary $error remains the actual passive concrete payload. Related originals
are projected when attached while their values are still available; their pinned formatters
may supply bounded previews under the shared formatter budget. Two reporter arguments therefore
provide all diagnostics without ambient "current exception" lookup or retaining whole related
objects. Exact related payloads requiring application retention should be recorded explicitly
while caught; diagnostic previews are not serialization or replay data.

Capture logical task frames at **first activation, before the producing call returns**, including
constructors, closures, accessors, conversions and callbacks. Native failures preserve the actual
Rust origin and separate dispatch boundary of the landed implementation. Scripted errors have
no fabricated Rust origin. Suspended calls retain their invocation frames. Internal invocation
IDs deduplicate capture/unwind merging while preserving recursion with identical names.

Rethrow preserves origin and adds propagation records. Relations form an immutable acyclic
diagnostic tree/DAG: prevent self/ancestor links and deduplicate occurrences. Capture at most
16 related occurrences, depth 4, 128 frames per occurrence and 64 KiB total diagnostic payload
preview. Truncation never discards the primary.

The prelude exports report_error($stack: CallStack, $error: ErrorValue): (). Its explicit
error-inclusive parameter borrows pending occurrences for observation instead of consuming
success like an any parameter would. Overrides must retain this parameter contract.
At activation, pin the creation site's visible
String::from_<concrete_type> factory by TypeID, method version and defining module environment;
do not capture producer-local bindings. Later imports/redefinitions do not change old failures'
default display. Explicit String conversion inside catch uses normal handler lookup.

~~~shy
pub fn String::from_apierror($error: ApiError): String
{
    "OpenAI HTTP ${error.status}: ${error.message}"
}
~~~

The prelude uses a guarded display intrinsic to invoke the pinned factory through its original
module authority. That capability is attached to CallStack, not reconstructed by name lookup
from the prelude. If error_type disagrees with the supplied payload, use structural fallback
rather than invoking an inapplicable factory.

The default reporter invokes that factory, otherwise decomposes public stored fields without
automatically running getters or exposing private fields. Formatting/snapshot work shares a
cumulative 100 ms / 50,000 VM instruction budget per packet, prohibits external commands,
resource operations and module writes, and discards incidental stdout. Exceptions, timeouts and wrong results trigger bounded structural
fallback while preserving the original error.

Print origin, native origin/boundaries, frames, propagation and related stacks to stderr.
Reporting calls never enter original frames. Limit output to 64 KiB with a 200 ms wait budget and one bounded native stderr worker. A failed reporter uses minimal native fallback without recursion. An overridden
root-visible report_error has the same computation guard, plus one explicit bounded stderr-write
capability; it cannot reopen files or call an external printer. Explicit successful reporting
returns () and observes rather than acknowledges a pending occurrence.

Uncaught failures unwind, clean up, report once and halt nonzero. Each REPL submission has an
implicit catch-all, reports, and leaves the REPL usable. Libraries do not print automatically.

### Public error groups

Export concrete marked structs and convenient named groups. The initial taxonomy is:

| Group | Expected members |
| --- | --- |
| io::Error | ReadError, WriteError, OpenError, ClosedError, TimeoutError, CancelledError, LimitError, DecodeError, LeaseError, ShortReadError |
| net::Error | Transport, HTTP, SSE, RPC and framing groups, preserving their concrete members |
| http::Error | StatusError, ProtocolError, TransportError, io::Error, json::Error |
| json::Error | SyntaxError, EncodingError, MappingError, PathError, LimitError, ValidationError |
| sse::Error | FrameError, io::Error, json::Error, http::Error |
| stream::Error | ErrorValue: application callback failures retain their original concrete type |
| rpc::Error | CallError, RequestError, ProtocolError, BatchError, RemoteError, http::Error, json::Error; streamed operations may also propagate io/framing failures |

Language-level Interrupted, CleanupAbort, CleanupTimeout, CleanupControl, ReceiverBusy,
OperationCancelled and UnsupportedSuspension are marked runtime failures with stable categories.
Cancellation control carries an internal uncatchable-during-cleanup flag; ordinary user
construction of an identically named error does not acquire that control authority.

Native interpreter failures also need a script-visible marked RuntimeError representation
that preserves their category and payload, with the existing trace supplied separately.
Specific native/library errors should not be flattened into RuntimeError when already typed.
Each I/O-consuming JSON function declares io::Error as well as json::Error.
The table describes expected library outcomes; user callbacks can propagate additional
concrete marked types and exceptions. A catch-all remains available for those extensions.
Native signature metadata must describe the groups rather than leave these APIs untyped.

## Shared I/O and resource ownership

### Binary data and endpoints

Bytes is immutable binary data. String-to-UTF-8 and byte-to-text conversion are explicit.
Readers and writers have the following conceptual operations:

| Operation | Success |
| --- | --- |
| Reader.read(max_bytes) | Bytes or () at EOF |
| Reader.read_exact(count) | Bytes, or a short-read error containing bytes/count received |
| Reader.read_all(limit_bytes) | Bytes within an explicit or inherited finite limit |
| Reader.read_text(limit_bytes) | Strict decoded text within a finite limit |
| Writer.write_some(bytes) | Number of bytes written |
| Writer.write_all(bytes) | Complete write count |
| Writer.flush | () |
| close | (); idempotent release of the resource |
| io::copy(reader, writer, options?) | Transfer count |

Each fallible operation additionally returns its marked io::Error group. max_bytes must be
positive. Empty bytes are data; () is EOF. read_exact 0 completes without reading.
Native interrupted system calls can retry internally, but language interruption cancels.

A ByteSource adapts a reader to pipeline bytes; a ByteSink adapts a writer or operation.
File helpers return explicit adapters: io::read_file, io::write_file and io::utf8.
Opening a file writer does not promise atomic replacement; partial files survive failures.
An eventual atomic-file adapter must specify rename, durability and cleanup separately.

### Shared handles and leases

Socket, reader, writer, listener, body and client copies alias one native resource.
They do not rewind or duplicate a resource. Closing the resource through one alias affects
the other aliases. Native final-reference cleanup is a fallback, never the failure trigger.

An operation lease gives a pipeline one consumer and appropriate read/write exclusivity.
Competing consumers fail explicitly. In-memory documents and their iterators remain ordinary
replayable values. A network iterator is single-use despite iteration's private receiver copy.

Cancellation closes operation-owned resources and releases borrowed leases. A borrowed TCP
writer finishing a pipeline flushes without half-closing the connection. shutdown_write is
explicit. A protocol failure that desynchronizes a borrowed connection marks it unusable.
Client.close releases its pool and cancels outstanding operations; close-time cancellation
and idempotency follow the same at-most-once release rules.

Lease phases are Reserved -> Acquiring -> Active -> Quiescing -> Releasing -> Released.
Failure from any nonterminal phase enters Quiescing, cancels waits, then Releasing. Mark a
lease released before reporting its release error, so finally cannot invoke it twice. Partial
acquisition belongs to the registry even before a public handle exists. Owned and borrowed
leases use distinct release actions; protocol desynchronization still invalidates a borrowed
connection. A return of a streaming server body transfers its native lease into the response
operation before handler cleanup, rather than relying on a surviving incidental alias.

## Pipelines and managed item streams

### Value and byte paths

Typed stages retain ordinary per-item behavior:

~~~shy
let $responses: [std::net::http::Response]
$urls | std::net::http::get | $responses

let $profile: Profile
(std::net::http::get $url).require_success.body json:|:Profile $profile
~~~

Response is one value. Selecting body creates a byte source. Only a source expands
automatically; returning a reader or iterable from a later stage does not splice it into
the pipeline. Explicit format annotations continue to encode a whole source value.

~~~shy
$document |:json external_command
$records | std::json::encode_line | (std::net::http::upload $request)
~~~

The first encodes one document. The second exposes records through the ordinary source
iterator, encodes each as compact JSON plus a newline, and uploads one continuous body.

An upload sink initializes once, receives chunks, finishes at upstream EOF and emits one
Response to the next value boundary. It never sends one request per chunk.

~~~shy
let $response: std::net::http::Response
$document |:json (std::net::http::upload $request) | $response
~~~

Adapters declare whether they accept typed items or native bytes; an ordinary expression
returning a writer is not silently callable. The runtime recognizes explicit adapter
capabilities. Bare $ retains its current incoming-item meaning.

Marked failures stop ordinary stages, cancel resources and propagate independently of
ExecResult. External statuses retain existing aggregation rules. A successful external
stage cannot overwrite a marked failure. Output already emitted cannot be rolled back. Scalar destinations retain existing per-item
assignment, so a catch can observe the last successful assignment. Array destinations collect
privately and commit only after normal exhaustion and successful operation cleanup; a marked
failure leaves the original binding unchanged. This commit does not depend on external exit
success: ExecResult remains separate from data. Collector limits apply before adding each item.

### Scheduling requirements

Choose a **resumable VM with single-thread cooperative tasks**. This is a required interpreter
change; a native handle containing a closure is not the execution mechanism.

Each task saves instruction position, evaluation stack, call frames, lexical/error regions,
handlers, loop state, last_result, pipeline input and redirections/output capture. Heap values
and modules remain shared on one interpreter thread. No Rust worker runs Shelly code.

Native invocation returns Ready(value), Failed(packet), or Wait(wait_spec, owned_resume_state).
Wait releases Rust borrows and suspends the task at that invocation. Readiness or a bounded
worker completion puts it back on the runnable queue. A worker receives owned bytes/OS state,
not Rc language values, binding references or an Interpreter reference. Invocation/generation
IDs discard late completions after cancellation without writing stale receiver state.

Pipeline stages are tasks linked by bounded queues (64 KiB byte queues and 256 item slots,
subject also to the operation's retained-byte limit). Queue capacity suspends producers;
empty queues suspend consumers. Retained-byte accounting counts unique shared backing roots,
not the apparent size of Document views, and rejects an item whose own retained graph exceeds
the operation budget. Apply bounded node accounting before queuing; unknown unaccountable
objects need an explicit adapter instead of pretending they are zero bytes. Native process/HTTP engines continue servicing reads and
writes while script tasks wait, including early upload rejection. Runnable tasks receive
1,024-instruction or 1 ms quanta in fair round-robin order. Nested script calls suspend in
the same task rather than recursively polling a blocked interpreter frame.

Mutable script receivers are pinned to a task across suspension, with their base/index evaluated
once. Another task attempting to mutate that same binding raises ReceiverBusy before mutation;
it does not wait and create a lock cycle. Same-task nested calls are allowed. Shared native
handles have operation leases instead of this receiver lock: closing an alias can cancel a
pending read. Task-local redirections prevent one suspended callback from changing another's
stdin/stdout. Shared global mutations can interleave at suspension points; the API promises
neither parallel script execution nor atomic multi-statement transactions.

Native functions declare bounded Ready work or supported Wait capability. A legacy blocking
native is rejected with UnsupportedSuspension **before invocation** in pipeline, cleanup and
reporting tasks. CPU-heavy codecs yield/check budgets. A bounded worker pool may support an
owned OS operation; mere offloading is insufficient if cancellation would retain unbounded
workers/resources. Admitting an operation requires a bounded cancellation/disposal contract.

| Task state | Event | Next state |
| --- | --- | --- |
| Runnable | Native/queue wait | Waiting(saved continuation) |
| Waiting | Readiness or completion with current generation | Runnable |
| Waiting/Runnable | Failure or cancellation | Unwinding(completion record) |
| Unwinding | Script cleanup needs supported wait | Waiting(cleanup continuation and remaining budget) |
| Unwinding | Cleanup exhausted/complete | Finished; release operation registry |

Acquisition starts left-to-right. Register the source lease **before** evaluating a later sink
factory. Each resource factory registers partial native acquisition before it can fail.
On setup failure, unwind acquired/reserved leases in reverse order, even if a source alias
still exists. Adapter ownership metadata distinguishes owned resources (close) from borrowed
connections (release exclusivity); a still-live borrowed connection is not automatically closed.

The managed native-wait ABI now spans invocation, iteration and pipeline producers.
Legacy process commands retain their supervised synchronous contract and are rejected
before invocation inside managed byte producers. The acceptance traces below remain design
reasoning; the implementation reference lists the scenarios actually executed.

### Managed scripted readers

stream::Reader(next:, result:, close:, context:) creates shared native state over script
callbacks. next returns an item, marked failure or () for EOF. Wrap legitimate null items.
result computes the completion value after next first returns (); close releases resources.

| State | next | result | close |
| --- | --- | --- | --- |
| Open | Run one next callback | NotFinished | Cancel active wait, then close callback |
| Finishing | Busy; no competing callback | Busy | Cancel finishing, then release |
| Exhausted | () | Cached success | () after already completed release |
| Failed | Repropagate stored failure | Repropagate stored failure | Only new release failure, never replay original |
| Cancelled | CancelledError | CancelledError | Idempotent () after release |

First EOF enters Finishing: compute result, cache its checked success, then release. Failure of
result/release produces Failed, not a false successful EOF. One pipeline lease has exclusive
next/result consumption; the lease owner and its same-task nested callbacks may use internal
reader operations while unrelated aliases fail. Reentrant invocation of that reader raises ReaderBusy instead of
waiting on itself. Different readers can suspend and progress independently.

Unwind/cancel the active callback before invoking close; callbacks never overlap. Release and
close callbacks run at most once even if they fail. Failure snapshots preserve concrete errors
and CallStack. Repeated explicit next/result access is a new propagation with the same origin.
Repeated close never replays that original failure, avoiding duplicate exceptions in finally.

Native state clears next/result/close closures after release and caches only completion data
or diagnostics. Captured mutable bindings therefore survive receiver copies only while needed.
An operation registry holds release authority independently of reference counts, so cycles
cannot block active-operation cleanup. Explicit close uses that same release authority; the
root registry entry becomes inactive immediately and cannot later close it a second time. New readers/body operations register with their current creation region and the root
submission before publication. Normal region completion transfers still-live entries into its
parent; exceptional region exit cancels/releases its entries before the enclosing catch.
This is a registry transfer, not a search for handles in returned objects. Returned streaming
resources remain in escrow until cleanup succeeds. Thus failure while building a wrapper
after opening its underlying SSE reader closes that reader immediately, even if an alias was
stored elsewhere. Borrowed transport entries release leases rather than persistent connections.

The root registry is the fallback for published but unconsumed operation readers/bodies;
remaining entries are cancelled at script/REPL-submission end.
Live item readers consequently cannot be carried to the next REPL submission. Persistent
clients/connections can; an explicit new reader creates a new operation in that submission.

Typed item read_as rejects target types accepting () unless boxed:true is explicit. In boxed
mode every success is stream::Item(value: mapped_value), including null. If an any-valued hook
unexpectedly returns bare () in unboxed mode, raise MappingError rather than false EOF.
Whole-document HTTP read_as has no iterator sentinel and can return null normally.

The default root registry permits 256 active/unconsumed item readers. Exceeding it raises a
LimitError before acquisition. Persistent resources require close or caller finally;
native handle cycles do not substitute for lifecycle management. This bounds abandoned reader
retention without introducing a tracing heap ownership algorithm.

## JSON module

### Ordinary values and exact documents

| Entry point | Purpose |
| --- | --- |
| json::decode(text, options?) | Parse and materialize ordinary Shelly values |
| json::parse(text, options?) | Parse an exact Document |
| json::wrap(value, options?) | Convert a structured value into a Document |
| json::encode(value, options?) | Encode JSON text |
| json::convert(document_or_value, type, options?) | Checked typed mapping |
| json::decode_as(text, type, options?) | Parse and map into the requested type |
| json::read(reader, options?) | Read one complete Document |
| json::write(value, writer, options?) | Write incrementally |

These operations use one codec engine and return their success or marked JSON/I/O failures.
Strings passed to parse/decode are text, never paths or URLs. Strings passed to wrap/encode
remain JSON strings; already-encoded JSON must be parsed to embed its structure.
A document is one JSON value, including a scalar or a selected subtree, not just an object.

Document is immutable and not implicitly iterable. Its API includes kind, value,
require_string(pointer), require_boolean(pointer), require_integer(pointer),
at(pointer), find(pointer), has(pointer), array_items(pointer), members and number_text.
array_items returns replayable Document views over an existing array.
Wrong kinds and invalid pointers fail; find only treats absent locations as missing.

A present null is a Document of kind Null. A missing find result is bare ().
Converting the null document to ordinary values produces (). Document-wrapped array elements
ensure [1,null,2] visits three elements through the existing iterator sentinel.

Documents preserve exact number spellings and parsed member order, but not original whitespace
or escape spelling. Subtree views share storage; retaining a small view can retain the root.
An explicit detached copy must be available when retention matters.
Re-encoding is not a promise of original bytes for signatures.

### Numeric and syntax policies

Exact parsing stores bounded, validated decimal tokens without eagerly expanding exponents.
json::Number represents such a number when materializing exact values.

| Materialization mode | Policy |
| --- | --- |
| Native | Signed 64-bit Integer when appropriate, otherwise finite Float with documented rounding |
| Exact | Fitting integer tokens become Integer; other number tokens remain json::Number |

Native conversion rejects overflow and underflow to zero; ordinary finite decimal-to-Float
rounding remains deliberate native behavior. Conversion to an integer requires an exact
integral value in range. No numeric mode silently converts a number to a String.

Duplicate names are rejected by default, including names equal after escape decoding.
Explicit FirstWins and LastWins compatibility modes are possible. Default parsing requires
strict UTF-8 and Unicode scalar strings; unpaired surrogates fail. Network writers do not add
a BOM. BOM acceptance, comments and trailing commas require separately named policies/formats.

Exactly one finite document is consumed and trailing non-whitespace input fails. Empty input
is invalid JSON; null is not an empty HTTP body. Object order is not semantic equality.

These choices follow JSON's interoperability constraints while deliberately selecting stricter
duplicate and Unicode policies than its permissive grammar:
[JSON specification](https://www.rfc-editor.org/rfc/rfc8259.html).

### Protocols and codec descriptions

~~~shy
pub fn User::to_json(): std::json::Document | std::json::Error
{
    std::json::wrap ["name": $self.name]
}

pub fn User::from_json($document: std::json::Document): User | std::json::Error
{
    User(name: $document.require_string "/name")
}
~~~

Encoding calls to_json on an isolated value receiver. Shared resource side effects cannot be
undone by that copy; custom codecs must avoid them. Decoding from_json is a factory without
$self. Required success types are checked; additional return alternatives must be marked.

Dispatch is directional: explicit to_json/from_json, then a type-level json_codec(): Codec,
then introspection. One custom direction need not override the other. Hook failures do not
trigger fallback. Document and Number are engine representations, not recursive user hooks.

A codec descriptor uses TypeIDs and immutable policy: enum wire names; struct renames,
omitted-null fields and unknown-field policy; reversible key mappings. Validate field references,
duplicate wire names, accessibility and type compatibility before reading a body. Pin provider
and method versions per operation. Providers run without resource operations or mutable module
writes, under a 100 ms / 100,000-instruction guard; recursive descriptor requests fail clearly.
Descriptors cannot grant access to private fields.

json_codec is a new receiver-free type-level provider. The interpreter supplies checked metadata
and caller-bound invocation capabilities; it does not give the JSON module privileged constructor
access merely because it possesses a Type object.

### Structural mapping

| Shelly type | Default JSON shape |
| --- | --- |
| String, Boolean, Integer, finite Float, () | Corresponding scalar or null |
| Array/fixed array | Recursive array; fixed lengths checked |
| String-keyed hash | Recursive object |
| Unit enum | Variant-name String |
| Enum-keyed hash | Variant names as keys; decode restores requested enum type |
| Struct | Public stored fields; decoding uses the checked construction interface |
| Named type | Underlying wire shape; typed conversion restores identity |
| Optional | Value or null |
| Union | One unambiguous compatible success member |

Hashes map directly to structs. Detect key collisions after enum renames/conversion. Other
non-string keys require an explicit reversible key codec or pairs representation, never a
display-string conversion. Strict fallback does not turn strings into numbers/booleans.
Required omissions fail; declared defaults/optional omissions apply only when missing; explicit
null is checked separately. Unknown fields fail unless a policy explicitly ignores them.

Selection must preserve ordinary source identity:

| Input to json::convert | Dispatch |
| --- | --- |
| Document | Target from_json; else target codec; else strict structural mapping |
| Ordinary typed value | Exact target from_<original_source_type>; else target from_json after wrap; else codec after wrap; else direct strict structural mapping |

No encode/parse round trip is needed. Named values and typed empty collections select their
original source factory before normalization. A custom factory controls its representation;
the engine validates the successful result and never retries another factory after failure.
Ordinary implicit field coercion does not parse a String as JSON.

Policy precedence is explicit operation option, then that target type's codec, then engine
default. Explicit options propagate to nested mapping; each nested codec supplies unspecified
options for its own type. A factory's internal explicit call is a new policy decision, but
inherits the operation's remaining hard budgets. No option bypasses access checks, constructor
invariants or runtime ceilings. Limits compose by minimum, never by resetting nested budgets.

Select union candidates with pure metadata/shape checks **before** invoking constructors.
If more than one candidate fits, or selection would require trying arbitrary custom factories,
raise AmbiguousUnion. Use an explicit enclosing from_json with a discriminator instead.
Invoke the selected constructor once. No speculative conversion side effects are allowed.

Introspection supplies public storage shape and accessible constructor parameter metadata.
Invoke construction under the original conversion caller's authority; neither a codec nor the
JSON wrapper grants private constructor access. Public computed getters need explicit hooks.
Without a factory, derived round trips work for compatible public fields. With a factory,
wire keys name its declared parameters. A different constructor/storage shape requires a hook
for a round trip. Unsupported private representations raise NoPublicRepresentation; a truly
empty public struct still encodes {}.

Conversion to a marked target activates a failure; objects merely containing an error field
do not. Passive recorded error snapshots may be encoded as public data, but decoding directly
into an error type is an explicit reactivation.

### Runtime Type results

json::convert(input, target: Type, options?) returns any | ErrorValue. The engine's own expected
failures are json::Error; a marked target or custom factory can return another concrete error.
This dynamic error outcome remains capturable, rather than being disguised as any success.
decode_as/read_as use the same dynamic result contract plus documented transport failures.
Before returning success, the conversion engine validates the result
against the requested TypeID, including factory output and nominal wrappers. Wrong hook results
become MappingError at the conversion boundary with the offending factory location, not an
apparently successful any value. The VM distinguishes generated factory-result contract
violations from user-thrown exceptions: JSON mapping wraps only the former as MappingError with
the contract failure as cause. Arbitrary callback exceptions propagate unchanged.

Literal Type arguments may enable compiler specialization; the contract does not require
dependent types. A success-only binding still checks the requested ordinary type. Typed JSON
pipeline destinations use this same engine. No API requires callers to annotate every binding
to trigger error promotion.

### Streaming JSON and limits

| Reader or adapter | Framing |
| --- | --- |
| json::read | One complete document |
| json::read_array(reader, pointer, options?) | Selected array elements as Documents |
| json::read_lines | One compact JSON document per physical line |
| json::read_sequence | Record-separator framed JSON texts |
| json::events | Structural events |
| json::array_source(items, options?) | One incrementally encoded JSON array |
| json::encode_line | One compact record plus LF |

Transport chunks are not records. Tokens may span chunks inside UTF-8, strings, escapes
and numbers. Decoder.feed/poll/finish distinguishes data, NeedInput and end. The initial single-document
Decoder retains bounded bytes and publishes a document after explicit finish. read_array
is the incremental selected-item adapter; events traverses an already parsed Document in
document member order. Neither API treats a transport chunk as a complete record.

JSON Lines rejects blank lines by default, accepts CRLF and a complete final line without LF.
Pretty multi-line JSON is not a line record. JSON Text Sequences use RS framing and expose
record errors explicitly; recovery never silently skips a failed record.
[Sequence framing](https://www.rfc-editor.org/rfc/rfc7464.html).

read_array emits elements before whole-document validity is known. Normal exhaustion validates
the remainder and trailing input. Early close cancels without claiming validity; an explicit
drain-and-validate operation is separate. Discarded subtrees are still syntax/duplicate checked.

The initial public limit profile is concrete and configurable:

| Budget | Default |
| --- | --- |
| Materialized document / encoded single document | 16 MiB |
| Framed record | 1 MiB |
| Nesting | 128 |
| String/scalar bytes | 1 MiB |
| Members/elements in one document | 100,000 total |
| Number token | 1,024 bytes; exponent magnitude at most 10,000 |
| Numeric comparison/canonicalization work | 1,000,000 work units |
| Custom hook work per operation | 5,000,000 VM instructions, 5,000 ms plus any earlier I/O deadline |
| Stream retained buffers | 16 MiB; individual pipeline queues remain smaller |

These are design defaults rather than benchmark claims. Runtime ceilings and explicit lower
limits compose by minimum. Framed streaming is bounded per record and retained working set,
not by silently buffering the whole response; a caller can add total byte/item/time limits.
Decoder work checks input-byte, member and depth budgets during discarded-subtree validation
too. Exact exponents cannot trigger unbounded expansion. Custom hooks suspend/check instruction
budgets, and recursion tracks (direction, TypeID, source identity); a recursive cycle fails while
distinct nested values are allowed within depth/work limits.

Document.detach copies the selected subtree under the same size limits. This releases the
original root once its other views expire. Diagnostic captures detach bounded previews rather
than retaining arbitrarily large document roots.

Array encoding emits [] for empty input. Interrupted encoders fail and may leave partial
bytes. Streaming writers cannot retroactively retract earlier output.

Pointer navigation, immutable set/remove/append, JSON Patch, Merge Patch and deterministic
diff form a later utility layer. Patch application is atomic over immutable documents.
Schema validation is separate from conversion; target a declared dialect, expose bounded issues,
do not coerce or insert defaults, and resolve references explicitly.
Canonicalization is a separate standards-based operation, not merely sorted keys; unsupported
exact numeric representations must fail rather than silently round.

## Networking module

### Transports and framing

TCP connections expose reader, writer, read/write operations, explicit shutdown_write and
close. Listeners expose accept and an explicit connection iterator.
Unix streams follow the same byte contract. TLS wraps a supported transport, validates
certificates and names by default, and exposes explicit trust configuration.

~~~shy
let $connection = std::net::tcp::connect "example.com" 9000
std::io::read_file "payload.bin" | $connection.writer
$connection.shutdown_write
$connection.reader | (std::io::write_file "reply.bin")
~~~

UDP preserves Datagram boundaries, peer addresses and truncation information. Empty datagrams
are valid. receive returns a Datagram or error; it does not treat an empty packet as EOF.
Low-level platform options stay transport-specific rather than leaking into every HTTP call.

Delimited text, fixed records, length-prefixed frames and custom codecs are reusable framing
adapters. Define maximum frame sizes, byte order, prefix widths and truncation handling.
A frame decoder retains partial input and preserves extra bytes after a completed frame.

### HTTP values and bodies

Request is inert configuration: method, path, ordered query pairs, headers, Body and
operation overrides. Client contains configuration and a shared connection pool; ordinary
methods retain Shelly's mutable-receiver semantics. Use one configured client for related
requests. Pool-sharing configuration forks are outside the initial public surface.

Response contains status, repeated headers, effective URL and one body resource.
The initial client exposes the effective URL; redirect-history inspection is deferred.
send/get/post return when final response headers are available. Body failure may occur later.
Redirect/retry configuration determines which exchanges are final and remains bounded.

~~~shy
let $api = std::net::http::Client(
    base_url: "https://example.com/api/",
    auth: std::net::http::bearer $TOKEN,
    timeout_ms: 30000,
)

let $response = $api.post "users" (std::net::http::json_body $user)
let $created: User = $response.require_success.read_as User
~~~

Body constructors explicitly select bytes, UTF-8 text, JSON, form, multipart or a reader.
json_body uses json::wrap/encoding and supplies application/json.
A conflicting caller content type fails unless an explicit override is selected.

Headers preserve ordered repeated fields with case-insensitive lookup. get requires a
singleton, get_all returns all matching values, and absent get returns ().
Do not combine fields such as Set-Cookie indiscriminately.
[HTTP field semantics](https://www.rfc-editor.org/rfc/rfc9110.html#section-5.2).

read_json, read_document, read_as and bytes/text reads consume the same body.
A second consumption fails. Partial manual reads followed by a whole-body decoder fail;
an explicit remaining-reader operation can decode just the unread tail.
A bounded buffer operation creates a replayable body, distinct from a live reader.
Whole-body read/decode operations own a registered consumption lease and release the body on
success or failure. A failed decoder cannot leak a half-consumed body into the pool.

Status is data until require_success checks it. Successful status means 2xx and the check
returns the same Response without consuming its body. No-content bodies are not JSON null.

Failed require_success atomically reserves an exclusive diagnostic lease. Capture at most
64 KiB under min(remaining operation deadline, 500 ms); an expired deadline permits no new
network read. Read one bounded lookahead byte when budget permits to distinguish an exact-limit
complete body from a truncated prefix. Close the live body; discard its pooled connection if
completion/reusability is uncertain. Aliases cannot race this read or subsequently consume it.

The target-type error decoder receives **StatusError** containing a DiagnosticResponse, not a
live Response. The replayable DiagnosticResponse contains status,
headers, URL/exchange metadata, delivery state, detached Bytes prefix, bytes received and
capture_complete/capture_reason. Reads are replayable. read_document refuses an incomplete
capture even if its prefix happens to be valid JSON. Bytes remain available after decoder
failure, without retaining a live socket.

error_decoder: DomainError selects DomainError::from_statuserror using the ordinary conversion
protocol; its marked target is the accepted replacement type. This preserves source cause
provenance automatically. A compatible target result becomes primary with StatusError as Cause.
Resolve and pin the decoder capability at client configuration under that caller's authority;
possession of a Type does not grant access to a private factory. A wrapper can configure its
own private conversion factory without making it public to the HTTP module or application.
A decoder throwing/returning a decoding failure leaves StatusError
primary, with the decoder failure related. Its callback return signature must identify the
domain error alternatives accepted as replacements; an arbitrary json::Error is not mistaken
for a successfully decoded domain failure. A successful non-error decoder result is a contract
failure attached to StatusError. StatusError always retains the bounded DiagnosticResponse.

The default URL client accepts relative references under its base and rejects credential-bearing
cross-origin requests unless explicitly enabled. Redirects strip origin-bound credentials across
origins, reject HTTPS downgrade by default, have a finite hop limit and validate every target.
Provide path-segment and query constructors; never interpolate unescaped input into URLs.

Headers/body aliases share resource state. The default client permits 64 open response bodies;
capacity exhaustion raises LimitError before sending another request rather than waiting forever
while a caller is collecting unconsumed Responses. Pool connection limits are separate.
Client.close cancels active operations and releases its pool. Ordinary one-shot calls must keep their owning transport alive through body consumption.

### Deadlines and delivery uncertainty

Use timeout_ms for one total operation budget and separate connect_timeout_ms and
idle_timeout_ms. Capture a monotonic deadline at operation start; redirects, retries, upload,
body reads and framing share the remaining budget. A later body read does not reset it.

Unbounded total duration requires explicit TimeoutPolicy::Unbounded configuration, with a finite idle policy retained
unless explicitly changed. Deadline behavior across client pooling and pagination is explicit:
each request has its own budget; a traversal can additionally have an overall budget.

Automatic retries are disabled initially. An explicit policy checks idempotence, delivery state,
replayability and remaining budget. One-shot bodies cannot be resent without a replay factory.
A timeout after transmission can leave delivery unknown; retain that fact in the error.
Do not automatically retry non-idempotent requests merely because no reply arrived.
[Retry semantics](https://www.rfc-editor.org/rfc/rfc9110.html#section-9.2.2).

Early HTTP responses during upload need concurrent protocol progress. Stop upstream production
on terminal rejection and preserve the response. Unread bodies are drained only within a
bounded policy; otherwise discard the pooled connection.

### REST and servers

REST helpers compose HTTP status checking, JSON conversion, typed endpoint functions and
service-specific error decoders. They do not duplicate parsers or mapping rules.

A paginator receives a Shelly callback that produces items and the next Request.
pages emits page records; items explicitly flattens their items. Fetch lazily, detect
repeated next links/cursors, enforce request/page limits and stop on cancellation.

HTTP servers use the same request/response/body objects, with a separate request-target policy.
Start with serialized Shelly handlers and bounded native connection queues. Handler exceptions
produce an appropriate server failure and run cleanup; exceptions are not serialized to peers
by default. Returning a streaming body retains its resource lease through transmission.
Concurrency of Shelly handlers and general async execution remain separate designs.

### SSE and JSON-RPC

SSE framing follows the [HTML event-stream parser](https://html.spec.whatwg.org/multipage/server-sent-events.html#parsing-an-event-stream),
with a deliberately strict UTF-8 profile. Accept LF, CRLF and lone CR; ignore one leading BOM;
join data lines with LF and remove the final joining LF. Comments and unknown fields do not emit
events. Dispatch only at a blank line with data. EOF discards an unterminated pending event;
it never supplies the missing blank delimiter. IDs containing NUL are ignored, retry accepts
ASCII digits only, and empty data lines still contribute a data field.

sse::read over an HTTP body checks text/event-stream before parsing; explicitly supplied raw
readers require the caller to choose SSE framing. Defaults: 1 MiB event, 16 KiB physical line,
16 KiB combined id/event/retry metadata. A malformed/truncated UTF-8 sequence fails rather than
using browser-style replacement. No automatic reconnect/replay, especially for generation POSTs.
Event contains data text, event name, id and optional retry; it does not interpret JSON.
read_as(Type) composes event-data parsing and typed conversion without changing framing.

RPC accepts HTTP, framed stream or a scripted exchange transport. call, notify, exchange and
batch are distinct; a pipeline of calls is not a batch. Use exact Documents to distinguish
missing id/result/error and valid null. Generated request IDs are strings. Correlation compares
JSON kind plus exact value, never display text. Version must be "2.0"; params must be an object
or array if present; replies have exactly one of result/error. Error code is an integer, message
a String, data optional. A null result is a successful Document, not iterator EOF.

| Exchange | Outcome |
| --- | --- |
| Empty batch | Local RequestError before transmission |
| call | Exactly one correlated reply or ProtocolError |
| notify | No RPC response promised; transport acceptance is not remote success |
| Mixed batch | Match every call exactly once; notifications have no response slots |
| Notification-only HTTP batch | Accept successful empty body; any nonempty RPC reply is ProtocolError |
| Duplicate, unexpected or missing reply ID | ProtocolError, preserving bounded response evidence |
| Invalid-batch single error with id:null | BatchError for the batch operation, not a missing-array error |
| Stream transport | Must supply explicit message framing; TCP chunks are not envelopes |

A scripted exchange transport declares whether notify completes without a reply or has an
explicit protocol acknowledgement; no generic read-until-reply is inserted. RPC server
dispatch implements the specification's invalid request/empty batch behavior separately from
client correlation. JSON-level syntax failures, envelope failures, transport failures and
valid remote errors remain distinct.

RemoteError retains code, message and optional data. BatchError, ProtocolError and RemoteError
join the public RPC union. Retry requires method semantics; HTTP POST alone cannot establish
safe replay. [JSON-RPC specification](https://www.jsonrpc.org/specification).

## OpenAI module example

The runnable sample is [examples/openai.shy](../examples/openai.shy), imported as openai
from an application's module search path. The excerpts below show its models and composition;
the source file is authoritative. It is not bundled as std::openai and tests make no live API calls.
Model selection remains a caller argument. The example supports text requests and preserves
the full response for other output kinds. It uses Responses with bearer authentication.

OpenAI output is heterogeneous, so collect text from all message content items rather than
assuming output[0].content[0]. [Response output](https://developers.openai.com/api/docs/guides/text).
HTTP request IDs use x-request-id. [Request metadata](https://developers.openai.com/api/reference/overview).
SSE terminal events distinguish completed, failed and incomplete responses.
[Event contract](https://developers.openai.com/api/reference/resources/responses/streaming-events).

### Module models and conversion

~~~shy
# examples/openai.shy (ordinary application module)
# Responses client. The model remains a caller choice; no live requests occur on import.
import std::io
import std::json
import std::stream
import std::net::http
import std::net::sse

pub enum Role { Developer, User, Assistant }

pub fn Role::json_codec(): std::json::Codec
{
    std::json::enum_codec Role [
        Role::Developer: "developer",
        Role::User: "user",
        Role::Assistant: "assistant",
    ]
}

pub struct Message
{
    pub role: Role,
    pub content: String,
}

pub struct Request
{
    pub model: String,
    pub input: String | [Message],
    pub instructions: optional String,
    pub max_output_tokens: optional Integer,
    pub store: optional Boolean,
}

pub fn Request::json_codec(): std::json::Codec
{
    std::json::struct_codec Request std::json::StructOptions(
        omit_null_fields: ["instructions", "max_output_tokens", "store"],
    )
}

pub struct Usage
{
    pub input_tokens: Integer,
    pub output_tokens: Integer,
    pub total_tokens: Integer,
}

struct Envelope
{
    pub id: String,
    pub status: String,
    pub usage: optional Usage,
}

pub struct Response
{
    pub id: String,
    pub status: String,
    pub usage: optional Usage,
    pub request_id: optional String,
    pub raw: std::json::Document,
}

pub fn Response::from_json($document: std::json::Document): Response | std::json::Error
{
    let $envelope: Envelope = std::json::convert $document Envelope (std::json::MappingOptions(unknown_fields: std::json::UnknownFields::Ignore))

    Response(
        id: $envelope.id,
        status: $envelope.status,
        usage: $envelope.usage,
        raw: $document,
    )
}

pub fn Response::to_json(): std::json::Document
{
    $self.raw
}

pub enum ResponseFailure { Incomplete, Failed, Refused, NoText, Protocol, Truncated }

pub error struct ApiError
{
    pub status: Integer,
    pub message: String,
    pub request_id: optional String,
    pub response: std::json::Document,
}

pub error struct ResponseError
{
    pub what: ResponseFailure,
    pub message: String,
    pub request_id: optional String,
    pub response: optional std::json::Document,
}

pub type Error = ApiError | ResponseError | std::net::http::Error | std::json::Error | std::net::sse::Error | std::stream::Error

pub fn String::from_apierror($error: ApiError): String
{
    "OpenAI HTTP ${error.status}: ${error.message}"
}

pub fn String::from_responseerror($error: ResponseError): String
{
    "OpenAI ${error.what}: ${error.message}"
}

# Return the marked value to transfer it to the caller's success-only context.
fn response_failure($what: ResponseFailure, $message: String,
                    $request_id: optional String,
                    $document: optional std::json::Document): ResponseError
{
    ResponseError(
        what: $what,
        message: $message,
        request_id: $request_id,
        response: $document,
    )
}

pub fn Response::require_text(): String | ResponseError | std::json::Error
{
    if $self.status != "completed"
    {
        let $what = if $self.status == "incomplete"
            { ResponseFailure::Incomplete }
            else { ResponseFailure::Failed }

        return (response_failure $what "Response did not complete" $self.request_id $self.raw)
    }

    let $text = ""
    for $item in $self.raw.array_items "/output"
    {
        if ($item.require_string "/type") != "message" { continue }

        for $part in $item.array_items "/content"
        {
            match ($part.require_string "/type")
            {
                "output_text" => { $text = $text + ($part.require_string "/text") },
                "refusal" =>
                {
                    return (response_failure ResponseFailure::Refused ($part.require_string "/refusal") $self.request_id $self.raw)
                },
                _ => {},
            }
        }
    }

    if $text == ""
    {
        return (response_failure ResponseFailure::NoText "Response contains no text output" $self.request_id $self.raw)
    }

    $text
}

fn ApiError::from_statuserror($error: std::net::http::StatusError): ApiError | std::json::Error | std::net::http::Error
{
    let $reply = $error.response
    let $document = $reply.read_document

    ApiError(
        status: $reply.status,
        message: ($document.require_string "/error/message"),
        request_id: ($reply.headers.get "x-request-id"),
        response: $document,
    )
}

~~~

Response::from_json maps routine fields through introspection while retaining unknown output.
The lower-level create operation returns an inspectable Response for incomplete results,
tool-only output and refusals. require_text is an explicit application policy.

### HTTP integration

~~~shy
pub struct Client
{
    transport: std::net::http::Client,
    model: String,
}

pub fn Client::new($api_key: String, $model: String, $base_url: optional String): Client | Error
{
    Client(
        transport: std::net::http::Client(
            base_url: (if $base_url == () { "https://api.openai.com/v1/" } else { $base_url }),
            auth: (std::net::http::bearer $api_key),
            timeout_ms: 120000,
            error_decoder: ApiError,
        ),
        model: $model,
    )
}

pub fn Client::create($request: Request): Response | Error
{
    let $reply: std::net::http::Response = $self.::transport.post "responses" (std::net::http::json_body $request)

    let $response: Response = $reply.require_success.read_as Response
    $response.request_id = $reply.headers.get "x-request-id"
    $response
}

pub fn Client::text($input: String, $instructions: optional String): String | Error
{
    let $response: Response = $self.create Request(
        model: $self.::model,
        input: $input,
        instructions: $instructions,
        store: false,
    )

    $response.require_text
}

pub fn Client::close()
{
    $self.::transport.close
}

~~~

error_decoder: ApiError selects ApiError::from_statuserror through the conversion
protocol. The target type supplies the replacement-error contract; thrown/returned JSON or
body failures remain decoder failures. A callback overload must declare the same target type
and accept the same StatusError source.
Client::new uses the factory-return rule defined above; this extends current constructors.
Callback-specific failures can be thrown despite generic HTTP signatures; this example's
exported group names its known API, HTTP, JSON and stream failures.

### Streaming integration

Expose two operations: events returns typed Events for tools/multiple channels; stream_text
is a convenience for one output-text channel. It rejects a second text channel rather than
flattening interleaved deltas in an order that differs from the completed response. create and
require_text still support aggregation of completed message content.

Use the conversion protocol at the event boundary:

~~~shy
pub struct Event
{
    pub kind: String,
    pub sequence: Integer,
    pub raw: std::json::Document,
}
pub fn Event::from_json($document: std::json::Document): Event | std::json::Error
{
    Event(kind: ($document.require_string "/type"),
        sequence: ($document.require_integer "/sequence_number"), raw: $document)
}

~~~

TextSession is module-private application protocol state, implemented in Shelly, not a proposed
OpenAI-aware Rust native. Its method contracts are part of this design exercise:

| Method | Contract |
| --- | --- |
| TextSession(request_id:) | Start AwaitCreated with no selected channel and an empty accumulator |
| accept(Event): optional String or Error | Apply the transitions below; return a delta or record a terminal transition |
| next(events): optional String or Error | Advance events until a delta/terminal; EOF before terminal raises Truncated |
| result(): Response or ResponseError | Return cached validated completion; NotFinished otherwise |

A successful completed transition stores the terminal Response. next subsequently returns
unit and result returns the cached response. An empty delta String remains real data.

| State/input | Validation and transition |
| --- | --- |
| Any nonterminal event | Nonnegative, strictly increasing sequence; reject duplicates/backtracking; gaps are allowed |
| AwaitCreated + response.created | Bind nonempty response.id; enter Active; duplicate creation fails |
| Before creation, other than error | Protocol failure before emission |
| output_item.added | Register bounded output_index -> item.id/type; duplicate index/id fails |
| content_part.added with output_text | Register the first (item.id, output_index, content_index); require known message item, empty initial text and no prior text channel |
| output_text.delta | Require exact registered tuple, open part and String delta; append under text limit, then emit delta |
| output_text.done | Require same tuple and text exactly equal to accumulated deltas; mark part text-done; duplicates fail |
| content_part.done / output_item.done | For selected text, require matching tuple/item and identical text; advance part/item lifecycle; reject late deltas/reopens |
| response.in_progress and other response-bearing events | Response ID must match; no second response can silently reuse this stream |
| response.completed | Matching response ID/status, selected part/item done, completed require_text equal to emitted accumulator; cache Response and finish |
| response.failed / response.incomplete / error | Raise typed failure; preserve request ID and bounded available response/event evidence |
| EOF before terminal | Truncated, including unterminated SSE terminal data |
| Known nontext/unknown extension event | Check sequence and any carried response ID; ignore only its nontext payload; never emit it |

For terminal events check kind/status consistency first. Refusal and no-text policy comes from
Response.require_text. Completed text is reconciled against the **whole** terminal aggregate,
so an extra text channel cannot be omitted while reporting success. Indexes are nonnegative;
the selected tuple must originate in added events, not merely first appear in a delta.
A malformed required field raises json::Error; a valid but contradictory event raises
ResponseFailure::Protocol. These are wire validation rules, not compiler appeasement checks.

Session limits are 256 output items, 32,768 events, 1 MiB accumulated UTF-8 text and 64 MiB
total decoded event bytes, composed with SSE/JSON limits and the request deadline. Reject before
emitting the chunk that crosses a limit. Validate unknown-event counts/sequence too. The first
valid terminal defines the end of this operation; close the transport rather than wait for a
possibly indefinite EOF. Data after that terminal is not consumed or claimed to be validated.

The pipeline-facing method stays small. TextSession implements the table above; its full SDK
implementation is intentionally outside this design-only change.

~~~shy
pub fn text_stream($events: std::stream::Reader, $request_id: optional String): std::stream::Reader | ErrorValue
{
    let $session = TextSession(request_id: $request_id)
    std::stream::Reader(
        next: fn (): optional String | Error { $session.next $events },
        result: fn (): Response | Error { $session.result },
        close: fn (): () | ErrorValue { $events.close },
    )
}
pub fn Client::events($request: Request): std::stream::Reader | ErrorValue
{
    let $body = std::json::set (std::json::wrap $request) "/stream" true
    let $reply = $self.::transport.post "responses" (std::net::http::json_body $body)
    $reply.require_success
    (std::net::sse::read $reply).read_as Event
}
pub fn Client::stream_text($request: Request): std::stream::Reader | ErrorValue
{
    let $body = std::json::set (std::json::wrap $request) "/stream" true
    let $reply = $self.::transport.post "responses" (std::net::http::json_body $body)
    $reply.require_success
    let $events = (std::net::sse::read $reply).read_as Event
    text_stream $events ($reply.headers.get "x-request-id")
}
~~~

read_as invokes Event::from_json automatically and transfers the underlying reader's
single-consumer lease. Mapping failure closes the source. Reader construction/acquisition is
registered before a later adapter can fail. Failed completion preserves already emitted bytes,
but .result never labels them a completed successful answer.

### Caller usage and reporting

~~~shy
import openai
import std::io

type WorkflowError = openai::Error | std::io::Error

let $ai = openai::Client(api_key: $OPENAI_API_KEY, model: $OPENAI_MODEL)

try
{
    let $answers: [String]
    ["Explain TCP.", "Explain UDP."] |
        $ai.text $ "Use one sentence." | $answers

    let $chunks = $ai.stream_text openai::Request(
        model: $OPENAI_MODEL,
        input: "Explain structured shell pipelines.",
        store: false,
    )
    $chunks | std::io::utf8 | (std::io::write_file "answer.txt")
    let $response = $chunks.result
    echo $response.id
}
catch ($stack: CallStack, $error: WorkflowError)
{
    report_error $stack $error
}
finally
{
    $ai.close
}
~~~

The pipeline acquires/owns the source operation before opening the sink, closes it on failure,
and releases it after completion. A second chunks.close finally is unnecessary; its cached
result remains available. Only the persistent client needs caller cleanup. Client construction
before try cannot make finally touch an unconstructed value; construction failures reach the
top-level reporter. Callback/runtime failures outside WorkflowError also reach that reporter.

For a script that needs no recovery, the caller can omit catch entirely and let the implicit
boundary report. Error handling remains a small fraction of the workflow. Adding a request,
field, conversion hook or output destination must not require a new per-call result check.

## Compatibility and implementation sequence

The design choices D1-D7 are now fixed contracts rather than unspecified implementation gates:

| Former gate | Resolution |
| --- | --- |
| D1 Responsibility | VM region ledger, scalar capture restrictions, explicit escrow/acknowledgement |
| D2 Type rules | ConsumeSuccess/Capture/Propagate/CatchFilter; checked factories and runtime Type results |
| D3 Trace/display | Activation-time frames; pinned formatter; explicit bounded related snapshots |
| D4 Cleanup | Completion records, cumulative budgets, cancellation generations and at-most-once release |
| D5 Streaming execution | Resumable single-thread VM tasks, native Wait ABI, bounded queues/registries |
| D6 Codecs | Original-source precedence, caller-bound metadata, pure union selection and ABI migration |
| D7 Protocols | DiagnosticResponse ownership, pinned SSE/RPC profiles and OpenAI TextSession transitions |

Implement foundations before public modules:

1. Language errors/contexts/regions, CallStack exposure, exception completion records and reporting.
2. Resumable invocation/iteration/redirection, native wait capability, cancellation and lease registry.
3. Exact JSON, strict conversion, checked metadata and codec protocols.
4. Transport and HTTP engines with local deterministic fixtures.
5. Framed JSON, SSE/RPC, pagination and application modules.
6. Schema, patch and canonicalization utilities after core behavior is established.

This is a coordinated compatibility release, **JSON protocol ABI 1**. String::from_json previously
encoded JSON text; that meaning is removed in this ABI, not selected by import order. Encoding
moves to json::encode/to_json. Every from_json hook must take exactly one Document parameter and
return its target or marked errors; old broad encoder signatures fail module validation with a
migration diagnostic. Move existing scripted String::from_json encoder overrides to to_json
or an explicit encoding function; a new String::from_json must decode a JSON string document.

The executable uses ABI 1 for its in-memory programs and format references. No serialized
compiled-program format is introduced here; future persistence must stamp and validate ABI versions.
A REPL cannot switch ABI mid-session. Within ABI 1, pinned method versions remain stable for an
existing operation; new operations can use a redefinition with the same required signature.
There is no temporary legacy encoder secretly eligible for new decoder dispatch.

CSV/TSV retain their own protocols/text mapping. Only JSON encoding boundaries stop looking up
the legacy String encoder. Update marked-result checks at every call, discard, assignment and
iteration boundary; current ExecResult behavior remains separate.

Use hidden native registrations behind script wrappers, directory/mod.rs for multi-file Rust
modules and Shelly behavioral tests. Do not modify target/release/shelly. Implementation and behavioral coverage accompany this revision; see the implementation
reference for commands, evidence and initial profile limits.

## Adversarial transition traces

These are paper traces with specified outcomes, not executed tests.

| Counterexample | Deterministic outcome |
| --- | --- |
| let $r = fail | ConsumeSuccess throws immediately into the nearest catch |
| let $r: Reply &#124; Error = fail; let $alias: Reply &#124; Error = $r; return $r | One escrowed token transfers; aliases expire without duplicate failure |
| Pending scalar assigned to outer/global/captured cell | Original failure promoted before mutation; outside storage unchanged |
| ["result": fail] followed by clearing the hash | First insertion promotes; no hidden obligation enters the hash |
| Borrow pending scalar into helper, inspect and return it | Owner remains caller; helper does not create a second occurrence |
| Two pending captures left at try-body exit | Earliest activation primary; second Unhandled; one catch acknowledges both |
| Pending root return plus another pending local | Returned failure primary; local attached Unhandled; no successful transfer |
| Pending root return and failed finally | Returned error primary; cleanup attached; no lost token |
| Catch stores Failure(stack:, error:), then later reads its error | Passive history; no spontaneous failure |
| Explicitly convert that history into domain error | New occurrence; original source Cause preserved |
| Convert a pending captured error and capture its replacement | Only replacement remains pending; original alias is passive Cause history |
| Named error-group catch receives FirstError | Concrete FirstError payload and TypeID; filter-union narrowing works |
| Callback suspends midway through SSE read | Saved frame/regions retained; other pipeline tasks and native I/O progress |
| Script callback reenters same reader | ReaderBusy rather than recursive wait/deadlock |
| Wrapper fails after opening SSE but before creating outer reader | Creation-region registry releases SSE/body before caller catch |
| Sink factory fails with live source alias retained | Already reserved source lease quiesces/releases; alias observes terminal state |
| Cancellation while read is waiting | Cancel wait generation, unwind callback, run close once under cleanup budget |
| Infinite loop in finally, then second Ctrl+C | Budget or CleanupAbort exits script cleanup; mandatory native release; REPL recovers |
| close in finally after original reader failure | No replay of original; only a new release failure can attach |
| Error normally returns through produce -> wrapper | Snapshot already contains produce and wrapper before their frames disappear |
| Handler/new formatter recursively fails | Original packet preserved; bounded structural/native fallback |
| HTTP 500 body is HTML or diagnostic prefix is cut off | StatusError primary with body prefix/flags and decoding cause; body closed |
| Duplicated/misidentified OpenAI delta | Sequence/identity or done/terminal reconciliation fails; result cannot claim success |

## Acceptance scenarios and remaining validation

| Area | Required behavioral fixtures |
| --- | --- |
| Errors | Bare/_ discard; inferred success; explicit capture; alias overwrite; return escrow; caught history |
| Catches | Nested named groups; concrete narrowing; TypeID redefinition; specific/group/all filters |
| Cleanup | Return/break/continue; helper exit; two release failures; cumulative budget; repeated Ctrl+C |
| Traces | Native origin vs boundary; normal returned error; conversion cause; recursion; reporting fallback |
| JSON | Missing/null; null iteration; number/exponent work; escaped duplicate names; width/depth limits |
| Mapping | Enum keys/collisions; original named/empty hash factory; private constructor; one-shot union selection |
| Streams | Split UTF-8; incomplete frame; late failure; early close; reentry; cached result; abandoned reader cap |
| Pipeline | Process/network duplex beyond queue capacity; task-local redirections; setup failure; early rejection |
| HTTP | Two consumers; partial read/decode; incomplete diagnostic capture; expired deadline; cross-origin redirects |
| RPC | Null result; absent fields; exact ID kinds; invalid/empty batches; notification-only body |
| OpenAI | Reasoning before text; refusal/tool-only; duplicate/gapped sequences; wrong tuple/ID; terminal mismatch; EOF |
| Usability | Endpoint plus typed codec without per-call checks; one workflow catch; no manual stack/cause plumbing |

The remaining work is empirical validation of the chosen contracts: VM suspension coverage,
platform cancellation/disposal behavior, and tuning limit defaults. Those are implementation
risks, not invitations to invent different observable semantics. Tests should challenge the
paper traces before exposing the corresponding public features.

Typed readers across REPL submissions, automatic HTTP retries and flattened multi-channel text
streaming are deliberately outside the initial contract. Their absence does not make callers
write additional error checks. A later extension should preserve the ordinary success path.

