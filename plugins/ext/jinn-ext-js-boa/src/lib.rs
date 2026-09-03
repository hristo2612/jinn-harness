//! The `jinn:ext` engine provider on Boa: a waterfall listener that runs
//! the operator's JS over every moment it is granted. Activation follows
//! the definition's law (`plugins/ext/jinn-ext/README.md`): the four
//! breadcrumbs in order, the source's sha256 on the record, the source
//! evaluated ONCE and the fiber failed unless it is a function, then one
//! `events.listen` per configured topic — a listen the kernel refuses
//! fails the activation, never swallowed. A delivery builds a fresh Boa
//! context (the spike's shape, "correct and slow" — its cost is proof 2's
//! measurement, and no reuse is designed before that number exists),
//! reads `jinn:clock` `now` once for the context's clock, applies the
//! source, and answers the folded JSON — or EMPTY bytes for `undefined`
//! (the kernel leaves the payload unchanged), or a contained fault for a
//! throw or a non-object (R9: recorded, the walk continues).
//!
//! The JS has NO host calls: the only imports of this component are
//! `types`, `effects`, `events` and `services` of `jinn:plugin@0.10.0`
//! (asserted by `tools/ext-kit`), and the one `services.call` targets a
//! kernel host provider, never a guest — so #4/#32's wait cycle has no
//! target here.

use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use boa_engine::context::time::FixedClock;
use boa_engine::{Context, Source};
use jinn_ext::{
    delivery, parse_config, self_test, source_breadcrumb, ExtConfig, BREADCRUMBS,
    CLOCK_CONTRACT, OP_NOW,
};

wit_bindgen::generate!({
    path: "../../../kernel-pin/wit",
    world: "plugin",
});

use exports::jinn::plugin::lifecycle::{Guest, GuestFault};
use jinn::plugin::{effects, events, services};

/// Effect tokens: the breadcrumbs, the source row, then one per listen.
const BREADCRUMB_TOKEN: u64 = 1;
const SOURCE_TOKEN: u64 = 10;
const LISTEN_TOKEN: u64 = 100;

static CONFIG: Mutex<Option<ExtConfig>> = Mutex::new(None);
/// The resolved clock handle for this incarnation (resolved once at
/// activation; each delivery is then exactly one crossing, the `now`).
static CLOCK: AtomicU64 = AtomicU64::new(0);

fn fault(context: &str, error: impl std::fmt::Debug) -> GuestFault {
    GuestFault::Failed(format!("{context}: {error:?}"))
}

fn breadcrumb(index: usize) -> Result<(), GuestFault> {
    effects::register(BREADCRUMBS[index], BREADCRUMB_TOKEN + index as u64)
        .map(drop)
        .map_err(|error| fault(BREADCRUMBS[index], error))
}

/// The one host read: `jinn:clock` `now`, 8-byte LE unix milliseconds.
fn now() -> Result<u64, GuestFault> {
    let handle = CLOCK.load(Ordering::SeqCst);
    let bytes = services::call(handle, OP_NOW, &[]).map_err(|error| fault(OP_NOW, error))?;
    let bytes: [u8; 8] = bytes
        .as_slice()
        .try_into()
        .map_err(|_| fault(OP_NOW, "not 8 bytes"))?;
    Ok(u64::from_le_bytes(bytes))
}

/// A fresh context on the kernel's clock. `Context::default()` would
/// build Boa's `StdClock` from `Instant::now()`, which has no
/// implementation in the plugin world and aborts before a word is said
/// (§5.4 lesson 1).
fn context(millis: u64) -> Result<Context, GuestFault> {
    Context::builder()
        .clock(Rc::new(FixedClock::from_millis(millis)))
        .build()
        .map_err(|error| fault("js context", error.to_string()))
}

fn activate(config: Vec<u8>) -> Result<(), GuestFault> {
    breadcrumb(0)?;
    let config = parse_config(&config).map_err(GuestFault::Failed)?;
    breadcrumb(1)?;
    let handle =
        services::resolve(CLOCK_CONTRACT).map_err(|error| fault(CLOCK_CONTRACT, error))?;
    CLOCK.store(handle, Ordering::SeqCst);
    let mut context = context(now()?)?;
    breadcrumb(2)?;
    let program = self_test(&config.source);
    let is_function = context
        .eval(Source::from_bytes(program.as_bytes()))
        .map_err(|error| fault("source", error.to_string()))?;
    if !is_function.as_boolean().unwrap_or(false) {
        return Err(GuestFault::Failed(
            "source: evaluates to something that is not a function".into(),
        ));
    }
    breadcrumb(3)?;
    effects::register(&source_breadcrumb(&config.source), SOURCE_TOKEN)
        .map_err(|error| fault("source breadcrumb", error))?;
    for (index, topic) in config.topics.iter().enumerate() {
        events::listen(topic, LISTEN_TOKEN + index as u64)
            .map_err(|error| fault(&format!("listen {topic}"), error))?;
    }
    *CONFIG.lock().unwrap() = Some(config);
    Ok(())
}

struct Boa;

impl Guest for Boa {
    fn activate(config: Vec<u8>) -> Result<(), GuestFault> {
        activate(config)
    }

    fn check(_consumer: u64) -> bool {
        true
    }

    fn undo(_token: u64) -> Result<(), GuestFault> {
        Ok(())
    }

    fn handle_event(token: u64, topic: String, payload: Vec<u8>) -> Result<Vec<u8>, GuestFault> {
        let source = {
            let config = CONFIG.lock().unwrap();
            let config = config.as_ref().ok_or_else(|| fault("delivery", "not active"))?;
            let index = usize::try_from(token.wrapping_sub(LISTEN_TOKEN)).unwrap_or(usize::MAX);
            if config.topics.get(index) != Some(&topic) {
                return Err(GuestFault::Failed(format!(
                    "unexpected event {topic:?} (token {token})"
                )));
            }
            config.source.clone()
        };
        let program = delivery(&source, &payload).map_err(GuestFault::Failed)?;
        let mut context = context(now()?)?;
        let answer = context
            .eval(Source::from_bytes(program.as_bytes()))
            .map_err(|error| fault("source", error.to_string()))?;
        let folded = answer
            .as_string()
            .map(|text| text.to_std_string_escaped())
            .ok_or_else(|| fault("source", "the fold did not stringify"))?;
        Ok(folded.into_bytes())
    }

    fn handle_call(
        _caller: u64,
        _contract: String,
        operation: String,
        _payload: Vec<u8>,
    ) -> Result<Vec<u8>, GuestFault> {
        // Nothing provides `jinn:ext`; a call here is a defect in the
        // caller's profile, refused loudly.
        Err(GuestFault::Failed(format!(
            "jinn-ext-js-boa provides no service (operation {operation:?})"
        )))
    }

    fn snapshot() -> Vec<u8> {
        Vec::new()
    }

    fn restore(_blob: Vec<u8>) -> Result<(), GuestFault> {
        Ok(())
    }
}

export!(Boa);

/// The `getrandom` custom backend (`getrandom_backend="custom"`; the
/// symbol is `__getrandom_v03_custom` even in 0.4, read from the crate's
/// `backends/custom.rs`). The plugin world imports no entropy and Boa
/// needs none for correctness — its hashers seed from here — so the
/// backend is a deterministic generator, stated as such.
static ENTROPY: AtomicU64 = AtomicU64::new(0x9E37_79B9_7F4A_7C15);

/// # Safety
///
/// `dest` is valid for `len` bytes (the caller's contract).
#[no_mangle]
unsafe extern "Rust" fn __getrandom_v03_custom(
    dest: *mut u8,
    len: usize,
) -> Result<(), getrandom::Error> {
    let mut state = ENTROPY.fetch_add(0x9E37_79B9_7F4A_7C15, Ordering::SeqCst);
    for offset in 0..len {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        // SAFETY: `offset < len` and `dest` is valid for `len` bytes.
        unsafe { dest.add(offset).write(state as u8) };
    }
    Ok(())
}
