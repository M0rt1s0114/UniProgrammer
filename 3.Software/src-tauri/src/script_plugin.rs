//! M3: sandboxed JavaScript plugin runtime for `kind = "protocol"` script
//! plugins.
//!
//! Every [`run_plugin`] call creates a fresh QuickJS runtime and context so
//! no JS state (globals, caches, prototypes) can leak across runs. Only the
//! QuickJS default built-ins plus the injected `uni` object are visible:
//! dynamic evaluation (`eval` / `Function`) and any Node/Web platform globals
//! are removed before the script executes.
//!
//! The chosen engine is `rquickjs` (QuickJS embedded in-process). Sandboxing
//! relies on QuickJS itself not shipping `require`, `fetch`, file, network or
//! timer APIs, plus explicit global removal and a resource limit set.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use rquickjs::{Array, Context, Ctx, Exception, Function, Null, Object, Runtime, Value};
use serde::Serialize;

use crate::hal_router::HalRouter;
use crate::plugin::{PluginKind, PluginManifest};

/// A single log line emitted by a script plugin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ScriptLogEntry {
    pub level: String,
    pub message: String,
}

/// A protocol registration emitted by `uni.register`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ScriptRegistration {
    pub id: String,
    pub kind: String,
    pub description: Option<String>,
}

/// Collected output from one script plugin run.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ScriptRunResult {
    pub logs: Vec<ScriptLogEntry>,
    pub registrations: Vec<ScriptRegistration>,
}

/// Interrupt scripts that run longer than this. QuickJS polls the interrupt
/// handler while executing bytecode, so tight loops are interrupted reliably.
const SCRIPT_TIMEOUT: Duration = Duration::from_millis(100);
/// Conservative in-process memory ceiling for one script run.
const SCRIPT_MEMORY_LIMIT: usize = 8 * 1024 * 1024;
/// Conservative stack ceiling (same as the QuickJS default).
const SCRIPT_MAX_STACK_SIZE: usize = 256 * 1024;
const TIMEOUT_MESSAGE: &str = "脚本执行超时或被资源限制中断";

/// Runs a single `kind = "protocol"` script plugin in a fresh sandboxed
/// QuickJS context.
pub fn run_plugin(manifest: &PluginManifest, source: &str) -> Result<ScriptRunResult, String> {
    run_plugin_inner(manifest, source, None)
}

/// Runs a script plugin with a live HAL router exposed as `uni.hal`.
pub fn run_plugin_with_hal(
    manifest: &PluginManifest,
    source: &str,
    hal: &mut HalRouter,
) -> Result<ScriptRunResult, String> {
    run_plugin_inner(manifest, source, Some(hal))
}

fn run_plugin_inner(
    manifest: &PluginManifest,
    source: &str,
    mut hal: Option<&mut HalRouter>,
) -> Result<ScriptRunResult, String> {
    if manifest.kind != PluginKind::Protocol {
        return Err(format!(
            "script plugins require kind = \"protocol\", got '{}'",
            manifest.kind
        ));
    }
    if !manifest.entry.ends_with(".js") {
        return Err(format!(
            "script plugin entry '{}' must end with \".js\"",
            manifest.entry
        ));
    }

    let logs = Rc::new(RefCell::new(Vec::new()));
    let registrations = Rc::new(RefCell::new(Vec::new()));

    let runtime =
        Runtime::new().map_err(|e| format!("failed to create JavaScript runtime: {e}"))?;
    runtime.set_memory_limit(SCRIPT_MEMORY_LIMIT);
    runtime.set_max_stack_size(SCRIPT_MAX_STACK_SIZE);
    let context =
        Context::full(&runtime).map_err(|e| format!("failed to create JavaScript context: {e}"))?;

    // The interrupt handler fires after the deadline and raises an
    // uncatchable exception, which is reported as a timeout/resource error.
    let deadline = Instant::now() + SCRIPT_TIMEOUT;
    let interrupted = Rc::new(Cell::new(false));
    let interrupted_flag = interrupted.clone();
    runtime.set_interrupt_handler(Some(Box::new(move || {
        if Instant::now() >= deadline {
            interrupted_flag.set(true);
            true
        } else {
            false
        }
    })));

    let run_logs = logs.clone();
    let run_registrations = registrations.clone();
    let result = context.with(|ctx| {
        harden_globals(&ctx)?;
        inject_uni(
            &ctx,
            manifest,
            run_logs,
            run_registrations,
            reborrow_hal(&mut hal),
        )?;
        ctx.eval::<(), _>(source)
            .map_err(|err| describe_script_error(&ctx, err, interrupted.get()))
    });
    runtime.set_interrupt_handler(None);

    result?;

    let collected_logs = logs.borrow().clone();
    let collected_registrations = registrations.borrow().clone();
    Ok(ScriptRunResult {
        logs: collected_logs,
        registrations: collected_registrations,
    })
}

/// Reborrows the HAL router option so `run_plugin_inner` keeps the original
/// `&mut HalRouter` alive until the freshly-created QuickJS context has been
/// dropped.
fn reborrow_hal<'a>(hal: &'a mut Option<&mut HalRouter>) -> Option<&'a mut HalRouter> {
    hal.as_deref_mut()
}

/// Removes unsafe globals before any plugin code runs.
fn harden_globals(ctx: &Ctx<'_>) -> Result<(), String> {
    let globals = ctx.globals();

    // Block dynamic evaluation. QuickJS exposes both `eval` and `Function`
    // as global functions in a full context. Removing `Function` alone is
    // not enough because `(function(){}).constructor` still reaches the
    // constructor, so remove that property before dropping the global.
    if globals.contains_key("eval").unwrap_or(false) {
        globals
            .remove("eval")
            .map_err(|e| format!("failed to remove eval global: {e}"))?;
    }
    if globals.get::<_, Function>("Function").is_ok() {
        let function_prototype = Function::prototype(ctx.clone());
        let _ = function_prototype.remove("constructor");
    }
    if globals.contains_key("Function").unwrap_or(false) {
        globals
            .remove("Function")
            .map_err(|e| format!("failed to remove Function global: {e}"))?;
    }

    // QuickJS has no Node/Web platform globals, but keep this defense in
    // depth in case a future engine build adds any of them.
    for forbidden in [
        "require",
        "fetch",
        "Deno",
        "process",
        "setTimeout",
        "setInterval",
        "clearTimeout",
        "clearInterval",
        "XMLHttpRequest",
        "WebSocket",
    ] {
        if globals.contains_key(forbidden).unwrap_or(false) {
            let _ = globals.remove(forbidden);
        }
    }

    Ok(())
}

/// Injects the `uni` SDK object into the global object.
fn inject_uni<'js>(
    ctx: &Ctx<'js>,
    manifest: &PluginManifest,
    logs: Rc<RefCell<Vec<ScriptLogEntry>>>,
    registrations: Rc<RefCell<Vec<ScriptRegistration>>>,
    hal_param: Option<&mut HalRouter>,
) -> Result<(), String> {
    let globals = ctx.globals();

    let uni = Object::new(ctx.clone()).map_err(|e| format!("failed to create uni: {e}"))?;
    uni.set("pluginApiVersion", 1u32)
        .map_err(|e| format!("failed to set uni.pluginApiVersion: {e}"))?;

    let manifest_obj =
        Object::new(ctx.clone()).map_err(|e| format!("failed to create uni.manifest: {e}"))?;
    manifest_obj
        .set("name", manifest.name.clone())
        .map_err(|e| format!("failed to set uni.manifest.name: {e}"))?;
    manifest_obj
        .set("version", manifest.version.to_string())
        .map_err(|e| format!("failed to set uni.manifest.version: {e}"))?;
    manifest_obj
        .set("kind", manifest.kind.as_str())
        .map_err(|e| format!("failed to set uni.manifest.kind: {e}"))?;
    uni.set("manifest", manifest_obj)
        .map_err(|e| format!("failed to set uni.manifest: {e}"))?;

    let log_fn = Function::new(ctx.clone(), {
        let logs = logs.clone();
        move |level: String, message: String| -> rquickjs::Result<()> {
            let level = match level.as_str() {
                "info" | "warn" | "error" => level,
                _ => "info".to_string(),
            };
            logs.borrow_mut().push(ScriptLogEntry { level, message });
            Ok(())
        }
    })
    .map_err(|e| format!("failed to create uni.log: {e}"))?;
    log_fn
        .set("info", log_level_fn(ctx, logs.clone(), "info")?)
        .map_err(|e| format!("failed to create uni.log.info: {e}"))?;
    log_fn
        .set("warn", log_level_fn(ctx, logs.clone(), "warn")?)
        .map_err(|e| format!("failed to create uni.log.warn: {e}"))?;
    log_fn
        .set("error", log_level_fn(ctx, logs.clone(), "error")?)
        .map_err(|e| format!("failed to create uni.log.error: {e}"))?;
    uni.set("log", log_fn)
        .map_err(|e| format!("failed to set uni.log: {e}"))?;

    let register_fn = Function::new(ctx.clone(), {
        let logs = logs.clone();
        let registrations = registrations.clone();
        move |ctx: Ctx<'_>, descriptor: Object<'_>| -> rquickjs::Result<bool> {
            let id: Option<String> = descriptor.get("id")?;
            let id = id.filter(|id| !id.is_empty());
            let Some(id) = id else {
                return Err(Exception::throw_type(
                    &ctx,
                    "uni.register: descriptor.id must be a non-empty string",
                ));
            };

            let kind: Option<String> = descriptor.get("kind").unwrap_or(None);
            let kind = kind.unwrap_or_default();
            if kind != "protocol" {
                logs.borrow_mut().push(ScriptLogEntry {
                    level: "warn".to_string(),
                    message: format!(
                        "uni.register: ignored registration for '{id}': unknown kind '{kind}'"
                    ),
                });
                return Ok(false);
            }

            let description: Option<String> = descriptor.get("description").unwrap_or(None);
            registrations.borrow_mut().push(ScriptRegistration {
                id,
                kind,
                description,
            });
            Ok(true)
        }
    })
    .map_err(|e| format!("failed to create uni.register: {e}"))?;
    uni.set("register", register_fn)
        .map_err(|e| format!("failed to set uni.register: {e}"))?;

    let hal = Object::new(ctx.clone()).map_err(|e| format!("failed to create uni.hal: {e}"))?;
    match hal_param {
        Some(router) => inject_hal(ctx, &hal, router)?,
        None => {
            hal.set("available", false)
                .map_err(|e| format!("failed to set uni.hal.available: {e}"))?;
            hal.set("call", Null)
                .map_err(|e| format!("failed to set uni.hal.call: {e}"))?;
        }
    }
    uni.set("hal", hal)
        .map_err(|e| format!("failed to set uni.hal: {e}"))?;

    globals
        .set("uni", uni)
        .map_err(|e| format!("failed to set global uni: {e}"))?;
    Ok(())
}

/// Injects the live `uni.hal` API backed by `router`.
fn inject_hal<'js>(
    ctx: &Ctx<'js>,
    hal: &Object<'js>,
    router: &mut HalRouter,
) -> Result<(), String> {
    hal.set("available", true)
        .map_err(|e| format!("failed to set uni.hal.available: {e}"))?;

    // `Function::new` requires its Rust closures to live as long as the JS
    // context (`'js`), so a plain `&mut HalRouter` borrow cannot be captured
    // directly. The borrow is erased into a raw pointer and kept alive by the
    // `hal` argument of `run_plugin_inner` for the whole synchronous run; the
    // functions and context are dropped before that borrow ends.
    let hal_cell = Rc::new(RefCell::new(router as *mut HalRouter));

    let adapters_fn = Function::new(ctx.clone(), {
        let hal_cell = hal_cell.clone();
        move |ctx: Ctx<'js>| -> rquickjs::Result<Array<'js>> {
            with_hal_router(&ctx, &hal_cell, "adapters", |router| {
                adapters_array(&ctx, router)
            })
        }
    })
    .map_err(|e| format!("failed to create uni.hal.adapters: {e}"))?;
    hal.set("adapters", adapters_fn)
        .map_err(|e| format!("failed to set uni.hal.adapters: {e}"))?;

    let open_fn = Function::new(ctx.clone(), {
        let hal_cell = hal_cell.clone();
        move |ctx: Ctx<'js>, adapter: String, device: String| -> rquickjs::Result<String> {
            with_hal_router(&ctx, &hal_cell, "open", |router| {
                router.open(&adapter, &device).map_err(|message| {
                    Exception::throw_type(&ctx, &format!("uni.hal.open: {message}"))
                })
            })
        }
    })
    .map_err(|e| format!("failed to create uni.hal.open: {e}"))?;
    hal.set("open", open_fn)
        .map_err(|e| format!("failed to set uni.hal.open: {e}"))?;

    let close_fn = Function::new(ctx.clone(), {
        let hal_cell = hal_cell.clone();
        move |ctx: Ctx<'js>, adapter: String, device: String| -> rquickjs::Result<Object<'js>> {
            with_hal_router(&ctx, &hal_cell, "close", |router| {
                router.close(&adapter, &device).map_err(|message| {
                    Exception::throw_type(&ctx, &format!("uni.hal.close: {message}"))
                })?;
                Object::new(ctx.clone())
            })
        }
    })
    .map_err(|e| format!("failed to create uni.hal.close: {e}"))?;
    hal.set("close", close_fn)
        .map_err(|e| format!("failed to set uni.hal.close: {e}"))?;

    let call_fn = Function::new(ctx.clone(), {
        let hal_cell = hal_cell.clone();
        move |ctx: Ctx<'js>,
              adapter: String,
              device: String,
              op: Object<'js>|
              -> rquickjs::Result<Object<'js>> {
            let write = parse_write_bytes(&ctx, &op)?;
            let read_len = parse_read_len(&ctx, &op)?;

            let data = with_hal_router(&ctx, &hal_cell, "call", |router| {
                router
                    .spi_transact(&adapter, &device, &write, read_len)
                    .map_err(|message| {
                        Exception::throw_type(&ctx, &format!("uni.hal.call: {message}"))
                    })
            })?;

            let result = Object::new(ctx)?;
            result.set("data", data)?;
            Ok(result)
        }
    })
    .map_err(|e| format!("failed to create uni.hal.call: {e}"))?;
    hal.set("call", call_fn)
        .map_err(|e| format!("failed to set uni.hal.call: {e}"))?;

    Ok(())
}

/// Runs `f` with a mutable HAL router borrow, rejecting nested re-entrant
/// calls as JS type errors instead of panicking through `RefCell`.
fn with_hal_router<'js, T>(
    ctx: &Ctx<'js>,
    hal_cell: &Rc<RefCell<*mut HalRouter>>,
    method: &str,
    f: impl FnOnce(&mut HalRouter) -> rquickjs::Result<T>,
) -> rquickjs::Result<T> {
    let mut borrowed = hal_cell.try_borrow_mut().map_err(|_| {
        Exception::throw_type(
            ctx,
            &format!("uni.hal.{method}: nested re-entrant HAL call is not supported"),
        )
    })?;
    // Safety: the pointer originates from the `&mut HalRouter` held by
    // `run_plugin_inner` for the entire synchronous script run, and the JS
    // functions / context are dropped before that borrow ends.
    let router = unsafe { &mut **borrowed };
    f(router)
}

/// Builds the JS `adapters()` array from the live HAL router.
fn adapters_array<'js>(ctx: &Ctx<'js>, router: &HalRouter) -> rquickjs::Result<Array<'js>> {
    let adapters = Array::new(ctx.clone())?;
    for (adapter_index, adapter) in router.adapters.iter().enumerate() {
        let devices = Array::new(ctx.clone())?;
        for (device_index, device) in adapter.devices.iter().enumerate() {
            let device_object = Object::new(ctx.clone())?;
            device_object.set("id", device.id.clone())?;
            device_object.set("kind", device.kind.clone())?;
            device_object.set("detail", device.detail.clone())?;
            devices.set(device_index, device_object)?;
        }

        let adapter_object = Object::new(ctx.clone())?;
        adapter_object.set("name", adapter.name.clone())?;
        adapter_object.set("devices", devices)?;
        adapters.set(adapter_index, adapter_object)?;
    }
    Ok(adapters)
}

/// Validates `op.write` and converts it into bytes.
fn parse_write_bytes(ctx: &Ctx<'_>, op: &Object<'_>) -> rquickjs::Result<Vec<u8>> {
    let value: Value = op.get("write")?;
    if !value.is_array() {
        return Err(Exception::throw_type(
            ctx,
            "uni.hal.call: op.write must be an array of byte numbers",
        ));
    }
    let array = value
        .as_array()
        .ok_or_else(|| Exception::throw_type(ctx, "uni.hal.call: op.write must be an array"))?;

    let mut write = Vec::with_capacity(array.len());
    for index in 0..array.len() {
        let item: Value = array.get(index)?;
        let number = item.as_number().ok_or_else(|| {
            Exception::throw_type(ctx, "uni.hal.call: op.write must contain only numbers")
        })?;
        if !number.is_finite() || number.fract() != 0.0 {
            return Err(Exception::throw_type(
                ctx,
                "uni.hal.call: op.write must contain only integers",
            ));
        }
        if !(0.0..=255.0).contains(&number) {
            return Err(Exception::throw_range(
                ctx,
                "uni.hal.call: op.write bytes must be in range 0..=255",
            ));
        }
        write.push(number as u8);
    }

    if write.len() > 4096 {
        return Err(Exception::throw_range(
            ctx,
            "uni.hal.call: op.write must be at most 4096 bytes",
        ));
    }
    Ok(write)
}

/// Validates `op.readLen` and converts it into a byte count.
fn parse_read_len(ctx: &Ctx<'_>, op: &Object<'_>) -> rquickjs::Result<usize> {
    let value: Value = op.get("readLen")?;
    let number = value
        .as_number()
        .ok_or_else(|| Exception::throw_type(ctx, "uni.hal.call: op.readLen must be a number"))?;
    if !number.is_finite() || number.fract() != 0.0 {
        return Err(Exception::throw_type(
            ctx,
            "uni.hal.call: op.readLen must be an integer",
        ));
    }
    if !(0.0..=65536.0).contains(&number) {
        return Err(Exception::throw_range(
            ctx,
            "uni.hal.call: op.readLen must be in range 0..=65536",
        ));
    }
    Ok(number as usize)
}

/// Creates one `uni.log.<level>(message)` convenience function.
fn log_level_fn<'js>(
    ctx: &Ctx<'js>,
    logs: Rc<RefCell<Vec<ScriptLogEntry>>>,
    level: &'static str,
) -> Result<Function<'js>, String> {
    Function::new(
        ctx.clone(),
        move |message: String| -> rquickjs::Result<()> {
            logs.borrow_mut().push(ScriptLogEntry {
                level: level.to_string(),
                message,
            });
            Ok(())
        },
    )
    .map_err(|e| format!("failed to create uni.log.{level}: {e}"))
}

/// Converts a JS engine error into a readable string, including the thrown
/// JavaScript error message when one is available.
fn describe_script_error(ctx: &Ctx<'_>, err: rquickjs::Error, interrupted: bool) -> String {
    if interrupted {
        return TIMEOUT_MESSAGE.to_string();
    }

    if err.is_exception() {
        let caught = ctx.catch();
        if caught.is_uncatchable_error() {
            return TIMEOUT_MESSAGE.to_string();
        }
        if let Some(exception) = caught.into_exception() {
            if let Some(message) = exception.message() {
                return message;
            }
        }
    }

    err.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn protocol_manifest(entry: &str) -> PluginManifest {
        PluginManifest::parse(&format!(
            r#"
[package]
name = "test-plugin"
version = "1.0.0"
plugin_api = 1
kind = "protocol"
entry = "{entry}"
"#
        ))
        .expect("test manifest should parse")
    }

    #[test]
    fn valid_script_logs_and_registers() {
        let manifest = protocol_manifest("plugin.js");
        let result = run_plugin(
            &manifest,
            r#"
                uni.log("info", "hello");
                uni.log.warn("careful");
                uni.log("debug", "normalized to info");
                uni.register({
                    id: "vnd.example.helloworld",
                    kind: "protocol",
                    description: "Example protocol plugin"
                });
            "#,
        )
        .expect("valid script should run");

        assert_eq!(result.logs.len(), 3);
        assert_eq!(result.logs[0].level, "info");
        assert_eq!(result.logs[0].message, "hello");
        assert_eq!(result.logs[1].level, "warn");
        assert_eq!(result.logs[1].message, "careful");
        assert_eq!(result.logs[2].level, "info");
        assert_eq!(result.logs[2].message, "normalized to info");

        assert_eq!(result.registrations.len(), 1);
        assert_eq!(result.registrations[0].id, "vnd.example.helloworld");
        assert_eq!(result.registrations[0].kind, "protocol");
        assert_eq!(
            result.registrations[0].description.as_deref(),
            Some("Example protocol plugin")
        );
    }

    #[test]
    fn sandbox_denies_require_fetch_and_dynamic_evaluation() {
        let manifest = protocol_manifest("plugin.js");
        let result = run_plugin(
            &manifest,
            r#"
                if (typeof require === 'undefined') {
                    uni.register({ id: "sandbox.no-require", kind: "protocol" });
                }
                if (typeof fetch === 'undefined') {
                    uni.register({ id: "sandbox.no-fetch", kind: "protocol" });
                }
                if (typeof eval === 'undefined') {
                    uni.register({ id: "sandbox.no-eval", kind: "protocol" });
                }
                if (typeof Function === 'undefined') {
                    uni.register({ id: "sandbox.no-function", kind: "protocol" });
                }
            "#,
        )
        .expect("sandbox marker script should run");

        let ids: Vec<&str> = result
            .registrations
            .iter()
            .map(|registration| registration.id.as_str())
            .collect();
        assert!(ids.contains(&"sandbox.no-require"));
        assert!(ids.contains(&"sandbox.no-fetch"));
        assert!(ids.contains(&"sandbox.no-eval"));
        assert!(ids.contains(&"sandbox.no-function"));
    }

    #[test]
    fn rejects_non_js_entry() {
        let manifest = protocol_manifest("plugin.py");
        let error = run_plugin(&manifest, "uni.log('info', 'x');").unwrap_err();
        assert!(error.contains("must end with \".js\""), "{error}");
    }

    #[test]
    fn rejects_non_protocol_kind() {
        let manifest = PluginManifest::parse(
            r#"
[package]
name = "adapter-plugin"
version = "1.0.0"
plugin_api = 1
kind = "adapter"
entry = "plugin.js"
"#,
        )
        .expect("test manifest should parse");
        let error = run_plugin(&manifest, "uni.log('info', 'x');").unwrap_err();
        assert!(error.contains("require kind = \"protocol\""), "{error}");
    }

    #[test]
    fn missing_register_id_throws_readable_error() {
        let manifest = protocol_manifest("plugin.js");
        let error = run_plugin(&manifest, "uni.register({ kind: 'protocol' });").unwrap_err();
        assert!(error.contains("uni.register"), "{error}");
        assert!(error.contains("id"), "{error}");
    }

    #[test]
    fn while_true_is_interrupted_by_resource_limit() {
        let manifest = protocol_manifest("plugin.js");

        // The interrupt handler is polled by QuickJS while it executes
        // bytecode; retry once or twice if the first attempt happens to slip
        // through before the deadline is observed.
        let mut unexpected = None;
        for attempt in 0..3 {
            match run_plugin(&manifest, "while(true){}") {
                Err(error) => {
                    assert_eq!(error, TIMEOUT_MESSAGE);
                    return;
                }
                Ok(result) => {
                    unexpected = Some(result);
                    if attempt < 2 {
                        std::thread::sleep(Duration::from_millis(20));
                    }
                }
            }
        }

        panic!(
            "while(true) script was not interrupted by the resource limit (last result: \
             {unexpected:?})"
        );
    }
}
