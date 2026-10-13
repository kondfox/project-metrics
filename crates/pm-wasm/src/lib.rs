//! The dashboard's metrics engine: the same `pm-metrics` code as the CLI, compiled to WASM, so a
//! custom range in the browser is computed exactly like a bucket in `project.json` (plan §2.2).
//!
//! The interface is JSON in, JSON out over a raw C ABI (no wasm-bindgen): the page writes a
//! request into memory from `pmx_alloc`, calls `pmx_call(ptr, len)`, and reads the response at
//! `ptr << 32 | len` (wasm32 pointers fit in 32 bits), which it releases with `pmx_free`.
//!
//! Requests:
//! - `{"op": "load", "days": {…}, "snapshots": {…}, "breadth_roles": […], "ai_attribution": bool,
//!   "pull_requests": meta.pull_requests, "velocity": velocity.options}` → `{"mix_roles": […]}`
//! - `{"op": "range", "start": "YYYY-MM-DD", "end": "YYYY-MM-DD"}` →
//!   `Metrics` (`{values, n, stack_mix, leaders}`; TypeScript type in crates/pmx/web/src/generated)
//! - The fleet page loads several projects: `load` and `range` then take a `"key"` (default `""`),
//!   `load` also `"velocity_basis"`, and `{"op": "fleet", "keys": […], "start", "end"}` returns the
//!   pooled fleet row (`Pooled`, pm-metrics `fleet`).
//!
//! - `{"op": "speedup", "key", "end": "YYYY-MM-DD", "range_start", "as_of", "timeline": project.timeline}`
//!   → `SpeedUps` (spec §4.1) for a range ending on `end`.
//!
//! Errors come back as `{"error": "…"}`.

use std::cell::RefCell;

use chrono::NaiveDate;
use pm_classify::Role;
use std::collections::BTreeMap;

use pm_metrics::fleet::{Member, pool};
use pm_metrics::model::{PullsInfo, mix_roles};
use pm_metrics::snapshot::SnapshotPoint;
use pm_metrics::speedup::{Timeline, speed_ups};
use pm_metrics::velocity::VelocityOptions;
use pm_metrics::{Days, MetricOptions};
use serde::Deserialize;
use serde_json::json;

thread_local! {
    /// Loaded projects by key (a project dashboard uses `""`).
    static STATE: RefCell<BTreeMap<String, Member>> = const { RefCell::new(BTreeMap::new()) };
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "lowercase")]
enum Request {
    Load {
        #[serde(default)]
        key: String,
        days: Days,
        #[serde(default)]
        snapshots: BTreeMap<NaiveDate, SnapshotPoint>,
        breadth_roles: Vec<Role>,
        ai_attribution: bool,
        #[serde(default)]
        pull_requests: Option<PullsInfo>,
        #[serde(default)]
        velocity: Option<VelocityOptions>,
        #[serde(default)]
        velocity_basis: Option<String>,
    },
    Range {
        #[serde(default)]
        key: String,
        start: NaiveDate,
        end: NaiveDate,
    },
    Fleet {
        keys: Vec<String>,
        start: NaiveDate,
        end: NaiveDate,
    },
    Speedup {
        #[serde(default)]
        key: String,
        end: NaiveDate,
        range_start: NaiveDate,
        as_of: NaiveDate,
        #[serde(default)]
        timeline: Timeline,
    },
}

/// Handle one JSON request.
pub fn handle(request: &str) -> String {
    let resp = match serde_json::from_str::<Request>(request) {
        Err(e) => json!({ "error": format!("bad request: {e}") }),
        Ok(Request::Load {
            key,
            days,
            snapshots,
            breadth_roles,
            ai_attribution,
            pull_requests,
            velocity,
            velocity_basis,
        }) => {
            let roles = mix_roles(&days);
            let reply = json!({ "mix_roles": roles });
            let member = Member {
                key: key.clone(),
                opts: MetricOptions {
                    breadth_roles,
                    ai_attribution,
                    mix_roles: roles,
                    pull_requests: pull_requests.is_some(),
                    pr_size: pull_requests.is_some_and(|p| p.sizes),
                    velocity,
                },
                days,
                snapshots,
                velocity_basis,
            };
            STATE.with(|s| s.borrow_mut().insert(key, member));
            reply
        }
        Ok(Request::Range { key, start, end }) => STATE.with(|s| match s.borrow().get(&key) {
            None => json!({ "error": "load the project first" }),
            Some(m) => serde_json::to_value(m.metrics(start, end)).expect("metrics serialize"),
        }),
        Ok(Request::Speedup {
            key,
            end,
            range_start,
            as_of,
            timeline,
        }) => STATE.with(|s| match s.borrow().get(&key) {
            None => json!({ "error": "load the project first" }),
            Some(m) => serde_json::to_value(speed_ups(&m.days, &m.opts, &timeline, range_start, end, as_of))
                .expect("speed-ups serialize"),
        }),
        Ok(Request::Fleet { keys, start, end }) => STATE.with(|s| {
            let s = s.borrow();
            match keys.iter().map(|k| s.get(k).ok_or(k)).collect::<Result<Vec<_>, _>>() {
                Err(k) => json!({ "error": format!("project `{k}` is not loaded") }),
                Ok(members) => serde_json::to_value(pool(&members, start, end)).expect("pooled serialize"),
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
        assert!(r["values"].get("velocity").is_none(), "not measured: left out");
    }

    #[test]
    fn fleet_pools_keyed_projects() {
        for key in ["a", "b"] {
            call(
                json!({"op": "load", "key": key, "days": days(), "breadth_roles": ["frontend", "backend"],
                "ai_attribution": true}),
            );
        }
        let a = call(json!({"op": "range", "key": "a", "start": "2025-01-01", "end": "2025-01-31"}));
        assert_eq!(a["values"]["active_devs"], json!(1.0));
        let f = call(json!({"op": "fleet", "keys": ["a", "b"], "start": "2025-01-01", "end": "2025-01-31"}));
        assert_eq!(f["values"]["commits"], json!(4.0));
        assert_eq!(f["values"]["active_devs"], json!(2.0), "p1 of a and p1 of b");
        assert_eq!(f["fed"]["quality"], json!(["a", "b"]));
        let missing = handle(
            &json!({"op": "fleet", "keys": ["a", "zz"], "start": "2025-01-01", "end": "2025-01-31"}).to_string(),
        );
        assert!(missing.contains("`zz` is not loaded"));
    }

    #[test]
    fn speed_up_for_any_range_end() {
        let load =
            json!({"op": "load", "key": "s", "days": days(), "breadth_roles": ["backend"], "ai_attribution": false});
        call(load);
        let r = call(
            json!({"op": "speedup", "key": "s", "end": "2025-02-28", "range_start": "2025-01-01",
            "as_of": "2025-03-01", "timeline": {"phases": [{"name": "dev", "from": "2025-01-06"}]}}),
        );
        assert_eq!(r["end"], "2025-02-28");
        assert_eq!(r["phase"]["label"], "dev");
        assert_eq!(r["phase"]["status"], "too_early", "{r}");
        assert_eq!(r["markers"], json!([]));
    }

    #[test]
    fn velocity_over_a_custom_range() {
        let mut d = days();
        d.get_mut(&NaiveDate::from_ymd_opt(2025, 1, 6).unwrap())
            .unwrap()
            .velocity
            .points = 6;
        d.get_mut(&NaiveDate::from_ymd_opt(2025, 1, 6).unwrap())
            .unwrap()
            .velocity
            .packages = 1;
        let load = json!({"op": "load", "days": d, "breadth_roles": ["backend"], "ai_attribution": false,
            "velocity": {"since": "2025-01-01", "as_of": "2025-03-01", "fte": [{"from": null, "to": null, "fte": 1.5}]}});
        call(load);
        // Mon 6 – Fri 10 January: 5 business days × 1.5 FTE.
        let r = call(json!({"op": "range", "start": "2025-01-06", "end": "2025-01-12"}));
        assert_eq!(r["values"]["velocity"], json!(6.0 / 7.5));
    }
}
