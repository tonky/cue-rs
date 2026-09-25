//! Operation-specific comparisons. The harness never evaluates CUE itself.
use crate::TxtarArchive;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub const ORACLE_REVISION: &str = "635e4bb441b29b0b8a3d754188b8edefe4012d1d";

#[derive(Debug, Deserialize)]
pub struct Manifest {
    pub oracle_revision: String,
    pub cases: Vec<Provenance>,
}

#[derive(Debug, Deserialize)]
pub struct Provenance {
    pub case: String,
    pub sha256: String,
    pub source_path: Option<String>,
    pub source_revision: Option<String>,
}

impl Manifest {
    pub fn verify(&self, case: &CaseResult) -> Result<(), String> {
        if self.oracle_revision != ORACLE_REVISION {
            return Err("manifest oracle revision mismatch".into());
        }
        let Some(entry) = self.cases.iter().find(|entry| entry.case == case.case) else {
            return Err("fixture is absent from the provenance manifest".into());
        };
        if entry.sha256 != case.sha256 {
            return Err("fixture bytes differ from recorded provenance".into());
        }
        if entry.source_path.is_some() && entry.source_revision.as_deref() != Some(ORACLE_REVISION)
        {
            return Err("fixture revision differs from the pinned oracle".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum Selector {
    Field(String),
    Index(usize),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Check {
    pub id: String,
    pub file: String,
    pub path: Vec<Selector>,
    pub operation: String,
    pub expected: Value,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    pub revision: String,
    pub sha256: String,
    pub checks: Vec<Check>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Passed,
    Mismatch,
    Unsupported,
    OracleMismatch,
    Crash,
    ResourceLimit,
    InvalidFixture,
    NotApplicable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckResult {
    pub id: String,
    pub operation: String,
    pub path: Vec<Selector>,
    pub status: Status,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseResult {
    pub case: String,
    pub sha256: String,
    pub checks: Vec<CheckResult>,
}

impl CaseResult {
    pub fn passed(&self) -> bool {
        self.checks.iter().any(|c| c.status == Status::Passed)
            && self
                .checks
                .iter()
                .all(|c| matches!(c.status, Status::Passed | Status::NotApplicable))
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Report {
    pub oracle_revision: String,
    pub cases: Vec<CaseResult>,
}

impl Report {
    pub fn accepted(&self) -> bool {
        !self.cases.is_empty() && self.cases.iter().all(CaseResult::passed)
    }

    /// Migration gate: match every case/check identity and outcome. It cannot
    /// be used as an acceptance report, and fixture changes invalidate it.
    pub fn compare_baseline(&self, baseline: &Self) -> Result<(), String> {
        if self.cases.is_empty() || baseline.cases.is_empty() {
            return Err("empty report cannot satisfy a baseline".into());
        }
        if self.cases.iter().flat_map(|c| &c.checks).any(|c| {
            matches!(
                c.status,
                Status::Crash | Status::ResourceLimit | Status::InvalidFixture
            )
        }) {
            return Err("infrastructure failures cannot satisfy a baseline".into());
        }
        if self.oracle_revision != baseline.oracle_revision {
            return Err("baseline oracle revision changed".into());
        }
        type BaselineIndex<'a> =
            BTreeMap<(&'a str, &'a str, &'a str), (&'a str, &'a [Selector], &'a Status)>;
        fn index(report: &Report) -> BaselineIndex<'_> {
            report
                .cases
                .iter()
                .flat_map(|case| {
                    case.checks.iter().map(move |check| {
                        (
                            (case.case.as_str(), case.sha256.as_str(), check.id.as_str()),
                            (
                                check.operation.as_str(),
                                check.path.as_slice(),
                                &check.status,
                            ),
                        )
                    })
                })
                .collect()
        }
        if index(self) == index(baseline) {
            Ok(())
        } else {
            Err("assertion baseline changed; review the per-assertion report (baseline matching is not conformance)".into())
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Request {
    pub directory: PathBuf,
    pub checks: Vec<Check>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", content = "value", rename_all = "snake_case")]
pub enum Observation {
    Value(Value),
    Unsupported(String),
    Error(String),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Response {
    pub observations: BTreeMap<String, Observation>,
}

pub struct Runner {
    pub oracle: PathBuf,
    pub worker: PathBuf,
    pub timeout_seconds: u64,
    pub scratch: PathBuf,
    pub manifest: Option<Manifest>,
}

impl Runner {
    pub fn run(&self, path: &Path) -> CaseResult {
        let mut case = CaseResult {
            case: path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into(),
            sha256: String::new(),
            checks: vec![],
        };
        if let Err((status, detail)) = self.run_inner(path, &mut case) {
            case.checks.push(CheckResult {
                id: "infrastructure".into(),
                operation: "load".into(),
                path: vec![],
                status,
                detail,
            });
        }
        case
    }

    fn run_inner(&self, path: &Path, case: &mut CaseResult) -> Result<(), (Status, String)> {
        let invalid = |e: std::io::Error| (Status::InvalidFixture, e.to_string());
        let data = fs::read(path).map_err(invalid)?;
        case.sha256 = Sha256::digest(&data)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        if let Some(manifest) = &self.manifest {
            manifest
                .verify(case)
                .map_err(|e| (Status::OracleMismatch, e))?;
        }
        let source =
            std::str::from_utf8(&data).map_err(|e| (Status::InvalidFixture, e.to_string()))?;
        let archive =
            TxtarArchive::parse(source).map_err(|e| (Status::InvalidFixture, e.to_string()))?;
        if archive.cue_files().is_empty() {
            return Err((Status::InvalidFixture, "no CUE inputs".into()));
        }
        fs::create_dir_all(&self.scratch).map_err(invalid)?;
        let temp = tempfile::tempdir_in(&self.scratch).map_err(invalid)?;
        let directory = fs::canonicalize(temp.path()).map_err(invalid)?;
        let input = directory.join("input");
        fs::create_dir(&input).map_err(invalid)?;
        for name in &archive.file_order {
            if name.starts_with("out/") {
                continue;
            }
            let file = input.join(name);
            fs::create_dir_all(file.parent().unwrap()).map_err(invalid)?;
            fs::write(file, &archive.files[name]).map_err(invalid)?;
        }
        let archive_path = fs::canonicalize(path).map_err(invalid)?;
        let output = self.command(
            &self.oracle,
            &["plan".as_ref(), archive_path.as_os_str(), input.as_os_str()],
            &directory,
            "oracle",
        )?;
        let plan: Plan = serde_json::from_slice(&output).map_err(|e| {
            (
                Status::OracleMismatch,
                format!("invalid oracle protocol: {e}"),
            )
        })?;
        if plan.revision != ORACLE_REVISION || plan.sha256 != case.sha256 {
            return Err((
                Status::OracleMismatch,
                "oracle revision or archive hash mismatch".into(),
            ));
        }
        if plan.checks.is_empty() {
            return Err((Status::Unsupported, "oracle returned no assertions".into()));
        }
        let mut ids = std::collections::HashSet::new();
        if plan.checks.iter().any(|c| !ids.insert(&c.id)) {
            return Err((Status::OracleMismatch, "duplicate assertion IDs".into()));
        }
        let requested: Vec<Check> = plan
            .checks
            .iter()
            .filter(|c| c.status.is_empty())
            .cloned()
            .collect();
        // Record all unsupported/oracle checks even if the evaluator crashes.
        for check in plan.checks.iter().filter(|c| !c.status.is_empty()) {
            let status = match check.status.as_str() {
                "unsupported" => Status::Unsupported,
                "not_applicable" => Status::NotApplicable,
                "oracle_mismatch" => Status::OracleMismatch,
                _ => {
                    return Err((
                        Status::OracleMismatch,
                        format!("unknown oracle status {}", check.status),
                    ));
                }
            };
            case.checks
                .push(result(check, status, check.reason.clone()));
        }
        if requested.is_empty() {
            return Ok(());
        }
        let request = Request {
            directory: input,
            checks: requested,
        };
        let request_path = directory.join("request.json");
        fs::write(&request_path, serde_json::to_vec(&request).unwrap()).map_err(invalid)?;
        match self.command(
            &self.worker,
            &["conformance-worker".as_ref(), request_path.as_os_str()],
            &directory,
            "worker",
        ) {
            Ok(output) => {
                let response: Response = serde_json::from_slice(&output)
                    .map_err(|e| (Status::Crash, format!("invalid worker protocol: {e}")))?;
                for check in &request.checks {
                    let outcome = match response.observations.get(&check.id) {
                        Some(Observation::Value(value))
                            if json_values_equal(value, &check.expected) =>
                        {
                            result(check, Status::Passed, String::new())
                        }
                        Some(Observation::Value(value)) => result(
                            check,
                            Status::Mismatch,
                            format!("expected {}, observed {value}", check.expected),
                        ),
                        Some(Observation::Error(error)) => {
                            result(check, Status::Mismatch, error.clone())
                        }
                        Some(Observation::Unsupported(reason)) => {
                            result(check, Status::Unsupported, reason.clone())
                        }
                        None => result(check, Status::Crash, "worker omitted observation".into()),
                    };
                    case.checks.push(outcome);
                }
            }
            Err((status, detail)) => {
                for check in &request.checks {
                    case.checks
                        .push(result(check, status.clone(), detail.clone()));
                }
            }
        }
        case.checks.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(())
    }

    fn command(
        &self,
        program: &Path,
        args: &[&std::ffi::OsStr],
        directory: &Path,
        label: &str,
    ) -> Result<Vec<u8>, (Status, String)> {
        let crash = |e: std::io::Error| (Status::Crash, e.to_string());
        let stdout = directory.join(format!("{label}.stdout"));
        let stderr = directory.join(format!("{label}.stderr"));
        // timeout controls the process group, including any descendant. The
        // enclosing `just safe` cgroup supplies the aggregate memory boundary.
        let status = Command::new("timeout")
            .args(["--kill-after=1s", &format!("{}s", self.timeout_seconds)])
            .arg(program)
            .args(args)
            .env("CUE_REGISTRY", "none")
            .env("CUE_EXPERIMENT", "")
            .env("CUE_DEBUG", "")
            .env("CUE_UPDATE", "")
            .stdin(Stdio::null())
            .stdout(File::create(&stdout).map_err(crash)?)
            .stderr(File::create(&stderr).map_err(crash)?)
            .status()
            .map_err(crash)?;
        if !status.success() {
            let state = if matches!(status.code(), Some(124 | 137)) {
                Status::ResourceLimit
            } else {
                Status::Crash
            };
            let detail = read_limited(&stderr).unwrap_or_default();
            return Err((
                state,
                format!(
                    "{label} exited {status}: {}",
                    String::from_utf8_lossy(&detail)
                ),
            ));
        }
        read_limited(&stdout).map_err(crash)
    }
}

fn read_limited(path: &Path) -> std::io::Result<Vec<u8>> {
    use std::io::Read;
    let mut data = Vec::new();
    File::open(path)?
        .take(16 * 1024 * 1024 + 1)
        .read_to_end(&mut data)?;
    if data.len() > 16 * 1024 * 1024 {
        return Err(std::io::Error::other("subprocess output exceeds 16 MiB"));
    }
    Ok(data)
}

fn result(check: &Check, status: Status, detail: String) -> CheckResult {
    CheckResult {
        id: check.id.clone(),
        operation: check.operation.clone(),
        path: check.path.clone(),
        status,
        detail,
    }
}

/// Canonical decimal without expanding an exponent or rounding through f64.
/// Input is a validated JSON number, or a Rust numeric value's Display output.
pub fn canonical_number(input: &str) -> Option<String> {
    use num_bigint::BigInt;
    let (mantissa, exponent) = input.split_once(['e', 'E']).unwrap_or((input, "0"));
    let mut exponent: BigInt = exponent.parse().ok()?;
    let negative = mantissa.starts_with('-');
    let mantissa = mantissa.strip_prefix('-').unwrap_or(mantissa);
    let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    if whole.is_empty()
        || !whole
            .bytes()
            .chain(fraction.bytes())
            .all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let digits = format!("{whole}{fraction}");
    let digits = digits.trim_start_matches('0');
    if digits.is_empty() {
        return Some("0e0".into());
    }
    let significant = digits.trim_end_matches('0');
    exponent += BigInt::from(digits.len() - significant.len());
    exponent -= BigInt::from(fraction.len());
    Some(format!(
        "{}{significant}e{exponent}",
        if negative { "-" } else { "" }
    ))
}

/// Compare JSON observations, ignoring object order and decimal spelling.
/// This does not implement CUE value equality, unification, or subsumption.
pub fn json_values_equal(left: &serde_json::Value, right: &serde_json::Value) -> bool {
    match (left, right) {
        (Value::Number(a), Value::Number(b)) => {
            match (
                canonical_number(&a.to_string()),
                canonical_number(&b.to_string()),
            ) {
                (Some(a), Some(b)) => a == b,
                _ => false,
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| json_values_equal(a, b))
        }
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len()
                && a.iter()
                    .all(|(k, v)| b.get(k).is_some_and(|w| json_values_equal(v, w)))
        }
        _ => left == right,
    }
}
