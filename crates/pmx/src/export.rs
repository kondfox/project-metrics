//! `pmx export --format long`: one row per `(metric, period)` (parity.md §1.1).

use pm_metrics::ProjectFile;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cadence {
    Weekly,
    Monthly,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub project: String,
    pub metric: String,
    pub period: String,
    pub value: Option<f64>,
    pub n: Option<u64>,
}

pub fn slug(name: &str) -> String {
    let mut s = String::new();
    for ch in name.to_lowercase().chars() {
        if ch.is_alphanumeric() {
            s.push(ch);
        } else if !s.ends_with('-') {
            s.push('-');
        }
    }
    s.trim_matches('-').to_string()
}

pub fn long_rows(p: &ProjectFile, cadence: Cadence) -> Vec<Row> {
    let project = slug(&p.project.name);
    let (labels, series, ns) = match cadence {
        Cadence::Weekly => (&p.weeks, &p.series_weekly, &p.n_weekly),
        Cadence::Monthly => (&p.months, &p.series_monthly, &p.n_monthly),
    };
    let mut rows = Vec::new();
    for (metric, values) in series {
        let n = ns.get(metric);
        for (i, (label, v)) in labels.iter().zip(values).enumerate() {
            rows.push(Row {
                project: project.clone(),
                metric: metric.clone(),
                period: label.clone(),
                value: *v,
                n: n.and_then(|n| n.get(i).copied().flatten()),
            });
        }
    }
    if let Some(ai) = &p.ai_compare {
        for (g, s) in [("ai", &ai.ai), ("human", &ai.human)] {
            let fields = [
                ("commits", Some(s.commits as f64)),
                ("added", Some(s.added as f64)),
                ("med_size", s.med_size),
                ("test_pct", s.test_pct),
                ("doc_pct", s.doc_pct),
            ];
            for (k, v) in fields {
                rows.push(Row {
                    project: project.clone(),
                    metric: format!("ai_compare_{g}_{k}"),
                    period: "last-12m".into(),
                    value: v,
                    n: None,
                });
            }
        }
    }
    rows
}

fn fmt_value(v: Option<f64>) -> String {
    match v {
        None => String::new(),
        Some(x) if x.fract() == 0.0 && x.abs() < 1e15 => format!("{}", x as i64),
        Some(x) => format!("{x}"),
    }
}

pub fn to_csv(rows: &[Row]) -> String {
    let mut out = String::from("project,metric,period,value,n\n");
    for r in rows {
        out.push_str(&format!(
            "{},{},{},{},{}\n",
            r.project,
            r.metric,
            r.period,
            fmt_value(r.value),
            r.n.map(|n| n.to_string()).unwrap_or_default()
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs() {
        assert_eq!(slug("Acme Shop"), "acme-shop");
        assert_eq!(slug("  Foo.com / Bar "), "foo-com-bar");
    }

    #[test]
    fn values() {
        assert_eq!(fmt_value(None), "");
        assert_eq!(fmt_value(Some(212.0)), "212");
        assert_eq!(fmt_value(Some(18.42)), "18.42");
    }
}
