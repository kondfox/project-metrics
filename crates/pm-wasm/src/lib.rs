//! The dashboard's metrics engine: the same `pm-metrics` code as the CLI, compiled to WASM, so a
//! custom range in the browser is computed exactly like a bucket in `project.json` (plan §2.2).
//!
//! The interface is JSON in, JSON out over a raw C ABI (no wasm-bindgen): the page writes a
//! request into memory from `pmx_alloc`, calls `pmx_call(ptr, len)`, and reads the response at
//! `ptr << 32 | len` (wasm32 pointers fit in 32 bits), which it releases with `pmx_free`.
//!
//! Requests:
//! - `{"op": "load", "days": {…}, "breadth_roles": […], "ai_attribution": bool}` → `{"mix_roles": […]}`
//! - `{"op": "range", "start": "YYYY-MM-DD", "end": "YYYY-MM-DD"}` →
//!   `Metrics` (`{values, n, stack_mix, leaders}`; TypeScript type in crates/pmx/web/src/generated)
//!
//! Errors come back as `{"error": "…"}`.

use std::cell::RefCell;

use chrono::NaiveDate;
use pm_classify::Role;
use std::collections::BTreeMap;

use pm_metrics::model::mix_roles;
use pm_metrics::snapshot::{SnapshotPoint, at_or_before};
use pm_metrics::{Days, MetricOptions, aggregate, metrics};
use serde::Deserialize;
use serde_json::json;

struct State {
    days: Days,
    snapshots: BTreeMap<NaiveDate, SnapshotPoint>,
    opts: MetricOptions,
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "lowercase")]
enum Request {
    Load {
        days: Days,
        #[serde(default)]
        snapshots: BTreeMap<NaiveDate, SnapshotPoint>,
        breadth_roles: Vec<Role>,
        ai_attribution: bool,
    },
    Range {
        start: NaiveDate,
        end: NaiveDate,
    },
}

/// Handle one JSON request.
pub fn handle(request: &str) -> String {
    let resp = match serde_json::from_str::<Request>(request) {
        Err(e) => json!({ "error": format!("bad request: {e}") }),
        Ok(Request::Load {
            days,
            snapshots,
            breadth_roles,
            ai_attribution,
        }) => {
            let roles = mix_roles(&days);
            let reply = json!({ "mix_roles": roles });
            STATE.with(|s| {
                *s.borrow_mut() = Some(State {
                    opts: MetricOptions {
                        breadth_roles,
                        ai_attribution,
                        mix_roles: roles,
                    },
                    days,
                    snapshots,
                })
            });
            reply
        }
        Ok(Request::Range { start, end }) => STATE.with(|s| match &*s.borrow() {
            None => json!({ "error": "load the project first" }),
            Some(st) => {
                let m = metrics(
                    &aggregate(&st.days, start, end),
                    &st.opts,
                    at_or_before(&st.snapshots, end),
                );
                serde_json::to_value(&m).expect("metrics serialize")
            }
        }),
    };
    resp.to_string()
}

/// Memory for a request of `len` bytes.
#[unsafe(no_mangle)]
pub extern "C" fn pmx_alloc(len: usize) -> *mut u8 {
    Box::into_raw(vec![0u8; len].into_boxed_slice()) as *mut u8
}

/// Release memory from [`pmx_alloc`] or a response.
///
/// # Safety
/// `ptr` and `len` must come from `pmx_alloc` or from a `pmx_call` result, and be freed once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pmx_free(ptr: *mut u8, len: usize) {
    if !ptr.is_null() {
        drop(unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(ptr, len)) });
    }
}

/// Handle the request at `ptr..ptr+len`; returns the response as `ptr << 32 | len`. Only
/// meaningful on wasm32, where pointers are 32-bit.
///
/// # Safety
/// `ptr..ptr+len` must be readable memory (from `pmx_alloc`).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pmx_call(ptr: *const u8, len: usize) -> u64 {
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    let reply = handle(&String::from_utf8_lossy(bytes)).into_bytes().into_boxed_slice();
    let len = reply.len() as u64;
    let out = Box::into_raw(reply) as *mut u8 as usize as u64;
    (out << 32) | len
}

#[cfg(test)]
mod tests {
    use super::*;

    use serde_json::Value;

    fn call(v: Value) -> Value {
        serde_json::from_str(&handle(&v.to_string())).unwrap()
    }

    fn days() -> Days {
        let mut days = Days::new();
        for (date, person, role, tech, n) in [
            ("2025-01-06", "p1", Role::Backend, "Go", 60u64),
            ("2025-01-07", "p1", Role::Frontend, "React", 50),
            ("2025-02-03", "p2", Role::Backend, "Go", 45),
        ] {
            let d = NaiveDate::parse_from_str(date, "%Y-%m-%d").unwrap();
            let day = days.entry(d).or_default();
            day.add_lines(person, role, tech, n);
            day.human.commits += 1;
            day.human.added += n;
            day.human.sizes.push(n);
            *day.person_commits.entry(person.into()).or_default() += 1;
        }
        days
    }

    #[test]
    fn load_then_range() {
        assert!(handle(r#"{"op":"range","start":"2025-01-01","end":"2025-01-31"}"#).contains("load the project first"));
        assert!(handle(r#"{"op":"nope"}"#).contains("bad request"));
        let load =
            json!({"op": "load", "days": days(), "breadth_roles": ["frontend", "backend"], "ai_attribution": true});
        assert_eq!(call(load)["mix_roles"], json!(["frontend", "backend"]));

        let jan = call(json!({"op": "range", "start": "2025-01-01", "end": "2025-01-31"}));
        assert_eq!(jan["values"]["commits"], json!(2.0));
        assert_eq!(jan["values"]["multi_stack_pct"], json!(100.0));
        assert_eq!(jan["values"]["test_discipline_pct"], Value::Null);
        assert_eq!(jan["leaders"][0]["name"], "p1");

        // A custom range across months: an exact roll-up of days, not of buckets.
        let r = call(json!({"op": "range", "start": "2025-01-07", "end": "2025-02-28"}));
        assert_eq!(r["values"]["commits"], json!(2.0));
        assert_eq!(r["values"]["added"], json!(95.0));
        assert_eq!(r["n"]["commit_med"], json!(2));
    }
}
